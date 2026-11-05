//! A project's own processes, started by the user on the Sites page: its Node app (a dev
//! server, or `node <file>`, reached at https://<project>.test through the web server's
//! proxy) and one-off commands (commands.rs). Unlike services they never start by
//! themselves: not with Start all, stack profiles or LampRabit's start (the user's choice,
//! 2026-10-31). Their output goes to the project's panel, logs/apps/<project>.log and the
//! Logs page.

use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::Mutex,
};

use crate::{
    config::{AppMode, NodeApp, PackageManager, RestartPolicy, Settings},
    diagnose,
    error::{Error, Result},
    logs,
    layout::{Layout, exe},
    node,
    procgroup::{self, ProcGroup},
    projects::{shell, strip_ansi},
    shims,
};

/// Output lines kept per app for the Sites page (the full output is in logs/apps/).
const LOG_LINES: usize = 500;
/// How long a starting app may take before its port answers.
const READY_TIMEOUT: Duration = Duration::from_secs(180);

fn package_json(dir: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(dir.join("package.json")).ok()?).ok()
}

/// The package manager a project uses: its lock file, then package.json's
/// "packageManager"; npm otherwise.
pub fn detect_package_manager(dir: &Path) -> PackageManager {
    let locks = [
        ("pnpm-lock.yaml", PackageManager::Pnpm),
        ("yarn.lock", PackageManager::Yarn),
        ("bun.lock", PackageManager::Bun),
        ("bun.lockb", PackageManager::Bun),
        ("package-lock.json", PackageManager::Npm),
    ];
    if let Some((_, pm)) = locks.iter().find(|(file, _)| dir.join(file).exists()) {
        return *pm;
    }
    let declared = package_json(dir).and_then(|p| p["packageManager"].as_str().map(str::to_string)).unwrap_or_default();
    [("pnpm@", PackageManager::Pnpm), ("yarn@", PackageManager::Yarn), ("bun@", PackageManager::Bun)]
        .into_iter()
        .find(|(prefix, _)| declared.starts_with(prefix))
        .map_or(PackageManager::Npm, |(_, pm)| pm)
}

/// package.json's scripts, for picking the command.
pub fn scripts(dir: &Path) -> BTreeMap<String, String> {
    package_json(dir)
        .and_then(|p| p["scripts"].as_object().cloned())
        .map(|m| m.into_iter().filter_map(|(k, v)| Some((k, v.as_str()?.to_string()))).collect())
        .unwrap_or_default()
}

fn depends_on(pkg: &serde_json::Value, name: &str) -> bool {
    ["dependencies", "devDependencies"].iter().any(|d| pkg[*d].get(name).is_some())
}

/// What a Node project is built with, from package.json; "Node" when nothing is known.
pub fn framework(dir: &Path) -> Option<&'static str> {
    let pkg = package_json(dir)?;
    let known = [
        ("next", "Next.js"),
        ("nuxt", "Nuxt"),
        ("astro", "Astro"),
        ("@sveltejs/kit", "SvelteKit"),
        ("@nestjs/core", "NestJS"),
        ("@remix-run/dev", "Remix"),
        ("vite", "Vite"),
        ("express", "Express"),
    ];
    Some(known.iter().find(|(dep, _)| depends_on(&pkg, dep)).map_or("Node", |(_, name)| name))
}

/// How a package manager's commands are spelled. pnpm and yarn run through npx when they
/// aren't installed for the project's Node version; bun has to be installed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmCommands {
    pub id: &'static str,
    /// `npm run`, `pnpm run`…
    pub run: String,
    /// What separates a script's own arguments: `--` for npm, nothing for the others.
    pub args: &'static str,
    /// Runs a package's binary without installing it: `npx -y`, `pnpm dlx`, `bunx`.
    pub exec: String,
    /// `npm create`, `pnpm create`…
    pub create: String,
    pub install: String,
    /// Adds a dependency: `npm install`, `pnpm add`…
    pub add: String,
}

/// Whether `program` is in one of the PATH folders (with Windows' .cmd/.exe forms).
pub fn on_path(program: &str, path: &OsString) -> bool {
    let names: Vec<String> = if cfg!(windows) {
        ["cmd", "exe", "ps1"].iter().map(|ext| format!("{program}.{ext}")).collect()
    } else {
        vec![program.to_string()]
    };
    std::env::split_paths(path).any(|dir| names.iter().any(|n| dir.join(n).is_file()))
}

pub fn pm_commands(pm: PackageManager, path: &OsString) -> Result<PmCommands> {
    // A missing pnpm or yarn runs through npx instead of failing.
    let via = |tool: &str| if on_path(tool, path) { tool.to_string() } else { format!("npx -y {tool}") };
    Ok(match pm {
        PackageManager::Npm => PmCommands {
            id: "npm",
            run: "npm run".into(),
            args: "--",
            exec: "npx -y".into(),
            create: "npm create".into(),
            install: "npm install".into(),
            add: "npm install".into(),
        },
        PackageManager::Pnpm => {
            let p = via("pnpm");
            PmCommands { id: "pnpm", run: format!("{p} run"), args: "", exec: format!("{p} dlx"), create: format!("{p} create"), install: format!("{p} install"), add: format!("{p} add") }
        }
        PackageManager::Yarn => {
            let y = via("yarn");
            PmCommands { id: "yarn", run: format!("{y} run"), args: "", exec: "npx -y".into(), create: format!("{y} create"), install: format!("{y} install"), add: format!("{y} add") }
        }
        PackageManager::Bun => {
            if !on_path("bun", path) {
                return Err(Error::msg("Bun isn't installed. Install it from bun.sh, or pick another package manager."));
            }
            PmCommands { id: "bun", run: "bun run".into(), args: "", exec: "bunx".into(), create: "bun create".into(), install: "bun install".into(), add: "bun add".into() }
        }
    })
}

/// PATH for a project's Node processes: its Node version first, then LampRabit's commands.
pub fn node_path(bin: &Path, layout: &Layout) -> OsString {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    std::env::join_paths([bin.to_path_buf(), shims::dir(layout)].into_iter().chain(std::env::split_paths(&existing))).unwrap_or(existing)
}

/// A first guess at how to run a project nobody has set up yet: its dev script (with the
/// port flag for the dev servers that ignore $PORT), else a server file.
/// Has something that starts a server: a dev/start script or a server file.
pub fn runnable(dir: &Path) -> bool {
    let scripts = scripts(dir);
    DEV_SCRIPTS.iter().any(|s| scripts.contains_key(*s)) || SERVER_FILES.iter().any(|f| dir.join(f).is_file())
}

const DEV_SCRIPTS: [&str; 4] = ["dev", "start:dev", "start", "serve"];
const SERVER_FILES: [&str; 6] = ["server.js", "index.js", "app.js", "main.js", "server.mjs", "index.mjs"];

/// How a project runs when nothing is saved yet, from its package.json. PHP projects
/// (Laravel with Vite...) get their asset dev server, which isn't served at the domain.
pub fn suggest(dir: &Path, port: u16) -> NodeApp {
    let pm = detect_package_manager(dir);
    let scripts = scripts(dir);
    // These dev servers ignore $PORT and $HOST; localhost can also mean IPv6 to them.
    let flag = matches!(framework(dir), Some("Vite" | "Astro" | "SvelteKit" | "Nuxt"));
    let separator = if pm == PackageManager::Npm { "--" } else { "" };
    let script = DEV_SCRIPTS.into_iter().find(|s| scripts.contains_key(*s));
    let file = SERVER_FILES.into_iter().find(|f| dir.join(f).exists());
    let php = crate::sites::language(dir) == crate::sites::Language::Php;
    let (mode, command) = match (script, file) {
        (Some(s), _) => {
            let port_arg = if flag && !php { format!(" {separator} --host 127.0.0.1 --port {{port}}").replace("  ", " ") } else { String::new() };
            (AppMode::Command, format!("{} run {s}{port_arg}", pm.id()))
        }
        (None, Some(_)) => (AppMode::File, String::new()),
        (None, None) => (AppMode::Command, format!("{} run dev", pm.id())),
    };
    NodeApp {
        mode,
        command,
        file: file.unwrap_or("server.js").to_string(),
        watch: true,
        port,
        node: None,
        package_manager: None,
        serve: !php,
    }
}

/// A free port from 3000 up that no other project's app or process uses.
pub fn free_port(settings: &Settings) -> u16 {
    let taken: Vec<u16> = settings
        .sites
        .values()
        .flat_map(|s| s.app.as_ref().map(|a| a.port).into_iter().chain(s.processes.iter().filter_map(|p| p.port)))
        .collect();
    (3000..4000).find(|p| !taken.contains(p) && std::net::TcpListener::bind(("127.0.0.1", *p)).is_ok()).unwrap_or(3000)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub name: String,
    /// stopped, starting (until the port answers), running, crashed.
    pub state: &'static str,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    /// Unix time in milliseconds the process started.
    pub since: Option<u64>,
    /// The Node version it runs on.
    pub node: Option<String>,
    pub exit_code: Option<i32>,
    /// A one-off job running for the project ("Installing dependencies").
    pub task: Option<String>,
}

impl AppStatus {
    fn stopped(name: &str) -> Self {
        Self { name: name.into(), state: "stopped", pid: None, port: None, since: None, node: None, exit_code: None, task: None }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AppOutput {
    pub name: String,
    pub line: String,
    /// "output", "error" (stderr) or "command" (what LampRabit ran, and how it ended).
    pub stream: &'static str,
}

/// A command run for a project, kept in its run history.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRun {
    pub label: String,
    pub command: String,
    /// Unix time in milliseconds it started.
    pub started: u64,
    pub duration_ms: u64,
    /// None when it was stopped.
    pub exit_code: Option<i32>,
}

/// A project's process (or dev server) that crashed: shown as an alert with its last lines.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Crash {
    pub name: String,
    pub exit_code: Option<i32>,
    pub lines: Vec<String>,
    /// Being restarted by its restart policy.
    pub restarting: bool,
    /// Crashed too often in a few minutes, so it's left stopped.
    pub gave_up: bool,
}

/// Runs kept per project.
const HISTORY: usize = 30;

/// Log files are trimmed to their last half when they pass this size.
const LOG_FILE_MAX: u64 = 2 * 1024 * 1024;

struct Entry {
    status: AppStatus,
    group: Option<ProcGroup>,
    /// The command running for the project, if any (one at a time).
    task_group: Option<ProcGroup>,
    task_run: u64,
    log: VecDeque<String>,
    /// Set by Stop, so an exit isn't reported as a crash.
    stopping: bool,
    /// Bumped per start, so tasks of an earlier run don't touch a newer one.
    run: u64,
    /// When a project process was restarted after exiting (ms), for `RESTARTS_IN_WINDOW`.
    restarts: Vec<u64>,
}

impl Entry {
    fn new(name: &str) -> Self {
        Self { status: AppStatus::stopped(name), group: None, task_group: None, task_run: 0, log: VecDeque::new(), stopping: false, run: 0, restarts: Vec::new() }
    }
}

/// A process that exits more often than this within `RESTART_WINDOW` is left stopped, so a
/// command that can't run doesn't loop forever.
const RESTARTS_IN_WINDOW: usize = 3;
const RESTART_WINDOW_MS: u64 = 5 * 60 * 1000;

/// The name a project's process goes by in statuses, logs and the Logs page: "shop/queue".
/// Folder names can't hold a slash, so it never clashes with a project.
pub fn process_key(project: &str, process: &str) -> String {
    format!("{project}/{process}")
}

/// Processes to start again after they exited, as (project, process). The restart needs
/// fresh settings, which the exit watcher doesn't have, so lib.rs's loop does it
/// (`restart_requests`).
static RESTARTS: std::sync::LazyLock<(tokio::sync::mpsc::UnboundedSender<(String, String)>, std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<(String, String)>>>)> =
    std::sync::LazyLock::new(|| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (tx, std::sync::Mutex::new(Some(rx)))
    });

/// The restart requests; can be taken once.
pub fn restart_requests() -> Option<tokio::sync::mpsc::UnboundedReceiver<(String, String)>> {
    RESTARTS.1.lock().ok()?.take()
}

#[derive(Default, Clone)]
pub struct Apps {
    entries: Arc<Mutex<HashMap<String, Entry>>>,
}

fn history_file(layout: &Layout, name: &str) -> PathBuf {
    layout.logs().join("apps").join(format!("{name}.runs.json"))
}

/// A project's recent command runs, newest first.
pub fn history(layout: &Layout, name: &str) -> Vec<CommandRun> {
    std::fs::read_to_string(history_file(layout, name)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn remember(layout: &Layout, name: &str, run: CommandRun) {
    let mut runs = history(layout, name);
    runs.insert(0, run);
    runs.truncate(HISTORY);
    if let Ok(text) = serde_json::to_string(&runs) {
        let _ = std::fs::write(history_file(layout, name), text);
    }
}

fn log_file(layout: &Layout, name: &str) -> PathBuf {
    // A process's "shop/queue" is a file next to the project's, not a folder.
    layout.logs().join("apps").join(format!("{}.log", name.replace('/', "~")))
}

impl Apps {
    pub async fn statuses(&self) -> Vec<AppStatus> {
        self.entries.lock().await.values().map(|e| e.status.clone()).collect()
    }

    pub async fn status(&self, name: &str) -> AppStatus {
        self.entries.lock().await.get(name).map_or_else(|| AppStatus::stopped(name), |e| e.status.clone())
    }

    /// Recent output; from the log file when the app hasn't run since LampRabit started.
    pub async fn logs(&self, layout: &Layout, name: &str) -> Vec<String> {
        if let Some(e) = self.entries.lock().await.get(name).filter(|e| !e.log.is_empty()) {
            return e.log.iter().cloned().collect();
        }
        let text = std::fs::read_to_string(log_file(layout, name)).unwrap_or_default();
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        lines[lines.len().saturating_sub(LOG_LINES)..].to_vec()
    }

    async fn update(&self, app: &AppHandle, name: &str, f: impl FnOnce(&mut Entry)) {
        let status = {
            let mut entries = self.entries.lock().await;
            let entry = entries.entry(name.to_string()).or_insert_with(|| Entry::new(name));
            f(entry);
            entry.status.clone()
        };
        let _ = app.emit("app-status", status);
        // Its domain may need to follow (to the dev server, or back to the files).
        crate::ports::poke();
    }

    /// An app still starting whose project now listens (perhaps not on the port it was
    /// given: some dev servers pick their own) is running, on that port.
    pub async fn mark_ready(&self, app: &AppHandle, name: &str, port: u16) {
        if self.status(name).await.state != "starting" {
            return;
        }
        self.update(app, name, |e| {
            if e.status.state == "starting" {
                e.status.state = "running";
                e.status.port = Some(port);
            }
        })
        .await;
    }

    /// Records a line of a project's output: its panel (memory and event), its log file
    /// and the Logs page.
    pub async fn record(&self, app: &AppHandle, layout: &Layout, name: &str, stream: &'static str, line: String) {
        let stamped = format!("[{}] {stream}: {line}", logs::stamp());
        {
            let mut entries = self.entries.lock().await;
            let entry = entries.entry(name.to_string()).or_insert_with(|| Entry::new(name));
            entry.log.push_back(stamped.clone());
            while entry.log.len() > LOG_LINES {
                entry.log.pop_front();
            }
        }
        if let Ok(mut f) = tokio::fs::OpenOptions::new().create(true).append(true).open(log_file(layout, name)).await {
            let _ = f.write_all(format!("{stamped}\n").as_bytes()).await;
        }
        let _ = app.emit("app-output", AppOutput { name: name.to_string(), line: stamped.clone(), stream });
        logs::push_project(app, name, stream, stamped);
    }

    /// Tells the UI a process crashed, with its last few lines of output.
    async fn report_crash(&self, app: &AppHandle, name: &str, exit_code: Option<i32>, restarting: bool, gave_up: bool) {
        let lines: Vec<String> = {
            let entries = self.entries.lock().await;
            let log = entries.get(name).map(|e| e.log.iter().filter(|l| !l.contains("] command: ")).cloned().collect::<Vec<_>>()).unwrap_or_default();
            log[log.len().saturating_sub(4)..].iter().map(|l| l.split_once(": ").map_or(l.as_str(), |(_, rest)| rest).to_string()).collect()
        };
        let _ = app.emit("process-crashed", Crash { name: name.to_string(), exit_code, lines, restarting, gave_up });
    }

    /// Streams a process's stdout or stderr into the project's output.
    fn pipe(&self, app: &AppHandle, layout: &Layout, name: &str, stream: &'static str, reader: impl tokio::io::AsyncRead + Unpin + Send + 'static) {
        let (apps, app, layout, name) = (self.clone(), app.clone(), layout.clone(), name.to_string());
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = strip_ansi(&line);
                if !line.trim().is_empty() {
                    apps.record(&app, &layout, &name, stream, line).await;
                }
            }
        });
    }

    /// Keeps a project's log file from growing without end: past LOG_FILE_MAX it keeps the
    /// newer half.
    fn trim_log(layout: &Layout, name: &str) {
        let path = log_file(layout, name);
        let Ok(len) = std::fs::metadata(&path).map(|m| m.len()) else { return };
        if len > LOG_FILE_MAX {
            if let Ok(bytes) = std::fs::read(&path) {
                let half = &bytes[bytes.len() / 2..];
                let start = half.iter().position(|b| *b == b'\n').map_or(0, |i| i + 1);
                let _ = std::fs::write(&path, &half[start..]);
            }
        }
    }

    pub async fn start(&self, app: &AppHandle, settings: &Settings, layout: &Layout, name: &str) -> Result<AppStatus> {
        let config = settings
            .sites
            .get(name)
            .and_then(|s| s.app.clone())
            .ok_or_else(|| Error::msg(format!("{name} has nothing to start: add a dev or start script to its package.json.")))?;
        if matches!(self.status(name).await.state, "starting" | "running") {
            return Ok(self.status(name).await);
        }
        let dir = layout.www().join(name);
        // Python and Java projects run their command with LampRabit's commands on PATH; Node
        // ones (and `node <file>`) with the project's Node version first.
        let node_project = dir.join("package.json").exists() || config.mode == AppMode::File;
        if std::net::TcpListener::bind(("127.0.0.1", config.port)).is_err() {
            let owner = diagnose::port_owner(config.port).await.map(|o| format!(" by {} (PID {})", o.name, o.pid)).unwrap_or_default();
            return Err(Error::msg(format!("Port {} is already in use{owner}. Stop that program, or choose another port for {name}.", config.port)));
        }
        let (version, bin, path) = if node_project {
            let (version, bin) = node::for_project(config.node.as_deref(), &dir)?;
            let path = node_path(&bin, layout);
            (version, bin, path)
        } else {
            (Default::default(), PathBuf::new(), Self::command_path(settings, layout, name))
        };
        let (program, args) = match config.mode {
            AppMode::Command => {
                let command = config.command.trim();
                if command.is_empty() {
                    return Err(Error::msg(format!("Enter the command that runs {name}, such as npm run dev.")));
                }
                shell(&command.replace("{port}", &config.port.to_string()))
            }
            AppMode::File => {
                if !dir.join(&config.file).is_file() {
                    return Err(Error::msg(format!("www/{name}/{} doesn't exist.", config.file)));
                }
                let mut args = Vec::new();
                if config.watch {
                    args.push("--watch".to_string());
                }
                args.push(config.file.clone());
                (bin.join(exe("node")), args)
            }
        };

        std::fs::create_dir_all(logs::project_dir(&layout.logs()))?;
        Self::trim_log(layout, name);
        let mut cmd = tokio::process::Command::new(&program);
        cmd.args(&args)
            .current_dir(&dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("PATH", &path)
            .envs(crate::shims::tool_env(settings, layout))
            .env("PORT", config.port.to_string())
            // Dev servers that read HOST bind where the proxy looks.
            .env("HOST", "127.0.0.1")
            // Dev servers otherwise try to open a browser or colour their output.
            .env("BROWSER", "none")
            .env("FORCE_COLOR", "0")
            .env("npm_config_yes", "true");
        procgroup::prepare(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| Error::msg(format!("Couldn't start {}: {e}", program.display())))?;
        let group = ProcGroup::attach(&child).ok();
        let pid = child.id();
        let since = diagnose::now_ms();
        let mut run = 0;
        self.update(app, name, |e| {
            e.run += 1;
            run = e.run;
            e.group = group;
            e.stopping = false;
            e.status = AppStatus { name: name.into(), state: "starting", pid, port: Some(config.port), since: Some(since), node: Some(version.clone()), exit_code: None, task: e.status.task.clone() };
        })
        .await;
        let shown = match config.mode {
            AppMode::Command => config.command.replace("{port}", &config.port.to_string()),
            AppMode::File => format!("node {}", args.join(" ")),
        };
        self.record(app, layout, name, "command", format!("Starting {shown} on Node {version}, port {}", config.port)).await;
        if let Some(out) = child.stdout.take() {
            self.pipe(app, layout, name, "output", out);
        }
        if let Some(err) = child.stderr.take() {
            self.pipe(app, layout, name, "error", err);
        }

        // Exit: stopped when asked, crashed otherwise.
        let (apps, handle, n, layout_c) = (self.clone(), app.clone(), name.to_string(), layout.clone());
        tauri::async_runtime::spawn(async move {
            let code = child.wait().await.ok().and_then(|s| s.code());
            let note = match code {
                Some(0) | None => "The app stopped".to_string(),
                Some(c) => format!("The app exited with code {c}"),
            };
            apps.record(&handle, &layout_c, &n, "command", note).await;
            let mut crashed = false;
            apps.update(&handle, &n, |e| {
                if e.run != run {
                    return;
                }
                e.group = None;
                e.status.state = if e.stopping || code == Some(0) { "stopped" } else { "crashed" };
                crashed = e.status.state == "crashed";
                e.status.exit_code = code;
                e.status.pid = None;
            })
            .await;
            if crashed {
                apps.report_crash(&handle, &n, code, false, false).await;
            }
        });
        // Ready: the port answers. A process that isn't served (an asset dev server) may
        // listen elsewhere, so it counts as running once started.
        if !config.serve {
            self.update(app, name, |e| {
                if e.run == run && e.status.state == "starting" {
                    e.status.state = "running";
                }
            })
            .await;
            return Ok(self.status(name).await);
        }
        let (apps, handle, n, port) = (self.clone(), app.clone(), name.to_string(), config.port);
        tauri::async_runtime::spawn(async move {
            let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
            while tokio::time::Instant::now() < deadline {
                let current = apps.entries.lock().await.get(&n).map(|e| (e.run, e.status.state));
                if current != Some((run, "starting")) {
                    return;
                }
                if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
                    apps.update(&handle, &n, |e| {
                        if e.run == run && e.status.state == "starting" {
                            e.status.state = "running";
                        }
                    })
                    .await;
                    return;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        });
        Ok(self.status(name).await)
    }

    /// Starts one of a project's processes (`SiteSettings.processes`) in the project folder,
    /// with the project's Node and LampRabit's commands on PATH. It's restarted after
    /// exiting when its policy says so, a few times in a row at most.
    pub async fn start_process(&self, app: &AppHandle, settings: &Settings, layout: &Layout, project: &str, process: &str) -> Result<AppStatus> {
        let key = process_key(project, process);
        let config = settings
            .sites
            .get(project)
            .and_then(|s| s.processes.iter().find(|p| p.name == process).cloned())
            .ok_or_else(|| Error::msg(format!("{project} has no process called {process}.")))?;
        if matches!(self.status(&key).await.state, "starting" | "running") {
            return Ok(self.status(&key).await);
        }
        let command = config.command.trim();
        if command.is_empty() {
            return Err(Error::msg(format!("Enter the command {process} runs, such as php artisan queue:work.")));
        }
        if let Some(port) = config.port {
            if std::net::TcpListener::bind(("127.0.0.1", port)).is_err() {
                let owner = diagnose::port_owner(port).await.map(|o| format!(" by {} (PID {})", o.name, o.pid)).unwrap_or_default();
                return Err(Error::msg(format!("Port {port} is already in use{owner}. Stop that program, or give {process} another port.")));
            }
        }
        let dir = layout.www().join(project).join(&config.cwd);
        if !dir.is_dir() {
            return Err(Error::msg(format!("{process} runs in {project}/{}, which doesn't exist. Pick another folder for it.", config.cwd)));
        }
        let path = Self::command_path(settings, layout, project);
        let command = match config.port {
            Some(port) => command.replace("{port}", &port.to_string()),
            None => command.to_string(),
        };
        let (program, args) = shell(&command);
        std::fs::create_dir_all(logs::project_dir(&layout.logs()))?;
        Self::trim_log(layout, &key);
        let mut cmd = tokio::process::Command::new(&program);
        cmd.args(&args)
            .current_dir(&dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("PATH", &path)
            .envs(crate::shims::tool_env(settings, layout))
            .env("HOST", "127.0.0.1")
            .env("BROWSER", "none")
            .env("FORCE_COLOR", "0")
            .env("npm_config_yes", "true")
            .envs(config.port.map(|p| ("PORT", p.to_string())))
            .envs(&config.env);
        procgroup::prepare(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| Error::msg(format!("Couldn't start {process}: {e}")))?;
        let group = ProcGroup::attach(&child).ok();
        let pid = child.id();
        let mut run = 0;
        self.update(app, &key, |e| {
            e.run += 1;
            run = e.run;
            e.group = group;
            e.stopping = false;
            let state = if config.port.is_some() { "starting" } else { "running" };
            e.status = AppStatus { name: key.clone(), state, pid, port: config.port, since: Some(diagnose::now_ms()), node: None, exit_code: None, task: None };
        })
        .await;
        self.record(app, layout, &key, "command", format!("$ {command}")).await;
        if let Some(out) = child.stdout.take() {
            self.pipe(app, layout, &key, "output", out);
        }
        if let Some(err) = child.stderr.take() {
            self.pipe(app, layout, &key, "error", err);
        }

        let (apps, handle, k, layout_c, project_c, process_c) = (self.clone(), app.clone(), key.clone(), layout.clone(), project.to_string(), process.to_string());
        tauri::async_runtime::spawn(async move {
            let code = child.wait().await.ok().and_then(|s| s.code());
            let stopping = apps.entries.lock().await.get(&k).is_some_and(|e| e.run == run && e.stopping);
            let again = !stopping
                && match config.restart {
                    RestartPolicy::Never => false,
                    RestartPolicy::OnFailure => code != Some(0),
                    RestartPolicy::Always => true,
                };
            // Room for another restart: fewer than RESTARTS_IN_WINDOW in the last few minutes.
            let now = diagnose::now_ms();
            let mut allowed = false;
            apps.update(&handle, &k, |e| {
                if e.run != run {
                    return;
                }
                e.group = None;
                e.status.state = if stopping || code == Some(0) { "stopped" } else { "crashed" };
                e.status.exit_code = code;
                e.status.pid = None;
                e.restarts.retain(|t| now.saturating_sub(*t) < RESTART_WINDOW_MS);
                if again && e.restarts.len() < RESTARTS_IN_WINDOW {
                    e.restarts.push(now);
                    allowed = true;
                }
            })
            .await;
            let note = match code {
                Some(c) => format!("{process_c} exited with code {c}"),
                None => format!("{process_c} stopped"),
            };
            let crashed = !stopping && code != Some(0);
            if !again {
                apps.record(&handle, &layout_c, &k, "command", note).await;
                if crashed {
                    apps.report_crash(&handle, &k, code, false, false).await;
                }
                return;
            }
            if !allowed {
                apps.record(&handle, &layout_c, &k, "error", format!("{note}; it exited {RESTARTS_IN_WINDOW} times in a few minutes, so it isn't restarted again. Check its output, then start it yourself.")).await;
                apps.report_crash(&handle, &k, code, false, true).await;
                return;
            }
            apps.record(&handle, &layout_c, &k, "command", format!("{note}; restarting it")).await;
            if crashed {
                apps.report_crash(&handle, &k, code, true, false).await;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
            // Stopped (or started by hand) during the pause: leave it be.
            if apps.entries.lock().await.get(&k).is_some_and(|e| e.run != run || e.stopping) {
                return;
            }
            let _ = RESTARTS.0.send((project_c, process_c));
        });

        // Ready once its port answers.
        if let Some(port) = config.port {
            let (apps, handle, k) = (self.clone(), app.clone(), key.clone());
            tauri::async_runtime::spawn(async move {
                let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
                while tokio::time::Instant::now() < deadline {
                    let current = apps.entries.lock().await.get(&k).map(|e| (e.run, e.status.state));
                    if current != Some((run, "starting")) {
                        return;
                    }
                    if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
                        apps.update(&handle, &k, |e| {
                            if e.run == run && e.status.state == "starting" {
                                e.status.state = "running";
                            }
                        })
                        .await;
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            });
        }
        Ok(self.status(&key).await)
    }

    /// Starts the processes that run with the project's app; the first error is returned
    /// after trying them all.
    pub async fn start_with_app(&self, app: &AppHandle, settings: &Settings, layout: &Layout, project: &str) -> Result<()> {
        let names: Vec<String> = settings.sites.get(project).map(|s| s.processes.iter().filter(|p| p.with_app).map(|p| p.name.clone()).collect()).unwrap_or_default();
        let mut first = None;
        for name in names {
            if let Err(e) = self.start_process(app, settings, layout, project, &name).await {
                self.record(app, layout, &process_key(project, &name), "error", e.to_string()).await;
                first.get_or_insert(e);
            }
        }
        first.map_or(Ok(()), Err)
    }

    /// Stops every process of a project (`with_app` only, or all of them).
    pub async fn stop_processes(&self, app: &AppHandle, settings: &Settings, project: &str, with_app_only: bool) {
        let prefix = process_key(project, "");
        let running: Vec<String> = self.entries.lock().await.keys().filter(|k| k.starts_with(&prefix)).cloned().collect();
        let own = settings.sites.get(project);
        for key in running {
            let name = &key[prefix.len()..];
            let with_app = own.and_then(|s| s.processes.iter().find(|p| p.name == name)).is_none_or(|p| p.with_app);
            if !with_app_only || with_app {
                self.stop(app, &key).await;
            }
        }
    }

    /// Asks the app to stop, then ends its whole process tree.
    pub async fn stop(&self, app: &AppHandle, name: &str) {
        let group = {
            let mut entries = self.entries.lock().await;
            let Some(e) = entries.get_mut(name) else { return };
            e.stopping = true;
            e.group.take()
        };
        let Some(group) = group else { return };
        if group.signal_term() {
            for _ in 0..50 {
                if !matches!(self.status(name).await.state, "starting" | "running") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        group.kill();
        self.update(app, name, |e| {
            if matches!(e.status.state, "starting" | "running") {
                e.status.state = "stopped";
                e.status.pid = None;
            }
        })
        .await;
    }

    /// Ends every app when LampRabit quits.
    pub async fn stop_all(&self) {
        for e in self.entries.lock().await.values_mut() {
            e.stopping = true;
            for g in [e.group.take(), e.task_group.take()].into_iter().flatten() {
                g.kill();
            }
        }
    }

    /// PATH for a project's commands: its Node version (when NVM has one), then LampRabit's
    /// commands, whose php and composer pick the project's PHP version by folder.
    pub fn command_path(settings: &Settings, layout: &Layout, name: &str) -> OsString {
        let dir = layout.www().join(name);
        let chosen = settings.sites.get(name).and_then(|s| s.app.as_ref()).and_then(|a| a.node.clone());
        match node::for_project(chosen.as_deref(), &dir) {
            Ok((_, bin)) => node_path(&bin, layout),
            Err(_) => {
                let existing = std::env::var_os("PATH").unwrap_or_default();
                std::env::join_paths(std::iter::once(shims::dir(layout)).chain(std::env::split_paths(&existing))).unwrap_or(existing)
            }
        }
    }

    /// Runs a command for a project (one at a time); its output goes to the project's log.
    /// Commands get no input, so ones that ask questions end instead of waiting.
    pub async fn run_command(&self, app: &AppHandle, settings: &Settings, layout: &Layout, name: &str, label: &str, command: &str) -> Result<()> {
        if let Some(task) = self.status(name).await.task {
            return Err(Error::msg(format!("\"{task}\" is still running for {name}. Wait for it or stop it first.")));
        }
        let dir = layout.www().join(name);
        let path = Self::command_path(settings, layout, name);
        let (program, args) = shell(command);
        std::fs::create_dir_all(logs::project_dir(&layout.logs()))?;
        Self::trim_log(layout, name);
        let mut cmd = tokio::process::Command::new(&program);
        cmd.args(&args)
            .current_dir(&dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("PATH", &path)
            .envs(crate::shims::tool_env(settings, layout))
            .env("FORCE_COLOR", "0")
            .env("NO_COLOR", "1")
            .env("npm_config_yes", "true")
            .env("COMPOSER_NO_INTERACTION", "1");
        procgroup::prepare(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| Error::msg(format!("Couldn't run {command}: {e}")))?;
        let group = ProcGroup::attach(&child).ok();
        let mut run = 0;
        self.update(app, name, |e| {
            e.task_run += 1;
            run = e.task_run;
            e.task_group = group;
            e.status.task = Some(label.to_string());
        })
        .await;
        self.record(app, layout, name, "command", format!("$ {command}")).await;
        if let Some(out) = child.stdout.take() {
            self.pipe(app, layout, name, "output", out);
        }
        if let Some(err) = child.stderr.take() {
            self.pipe(app, layout, name, "error", err);
        }
        let (apps, handle, n, layout, label, command) = (self.clone(), app.clone(), name.to_string(), layout.clone(), label.to_string(), command.to_string());
        let started = diagnose::now_ms();
        tauri::async_runtime::spawn(async move {
            let code = child.wait().await.ok().and_then(|s| s.code());
            remember(&layout, &n, CommandRun { label: label.clone(), command, started, duration_ms: diagnose::now_ms().saturating_sub(started), exit_code: code });
            // Let the last output lines land before the summary.
            tokio::time::sleep(Duration::from_millis(150)).await;
            let summary = match code {
                Some(0) => format!("{label} finished"),
                Some(c) => format!("{label} failed (exit code {c})"),
                None => format!("{label} was stopped"),
            };
            apps.record(&handle, &layout, &n, "command", summary).await;
            apps.update(&handle, &n, |e| {
                if e.task_run == run {
                    e.task_group = None;
                    e.status.task = None;
                }
            })
            .await;
        });
        Ok(())
    }

    /// Stops the command running for a project.
    pub async fn stop_command(&self, name: &str) {
        let group = self.entries.lock().await.get_mut(name).and_then(|e| e.task_group.take());
        if let Some(g) = group {
            g.kill();
        }
    }

    /// Installs the project's dependencies with its package manager.
    pub async fn install(&self, app: &AppHandle, settings: &Settings, layout: &Layout, name: &str) -> Result<()> {
        let dir = layout.www().join(name);
        let config = settings.sites.get(name).and_then(|s| s.app.clone()).unwrap_or_default();
        let path = Self::command_path(settings, layout, name);
        let pm = config.package_manager.unwrap_or_else(|| detect_package_manager(&dir));
        let commands = pm_commands(pm, &path)?;
        self.run_command(app, settings, layout, name, &format!("Installing dependencies with {}", commands.id), &commands.install).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(files: &[(&str, &str)]) -> PathBuf {
        // Tests run in parallel; each project gets its own folder.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lamprabit-app-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, text) in files {
            std::fs::write(dir.join(name), text).unwrap();
        }
        dir
    }

    #[test]
    fn package_manager_from_lock_file_then_package_json() {
        let pnpm = project(&[("package.json", "{}"), ("pnpm-lock.yaml", "")]);
        assert_eq!(detect_package_manager(&pnpm), PackageManager::Pnpm);
        let declared = project(&[("package.json", r#"{"packageManager":"yarn@4.1.0"}"#)]);
        assert_eq!(detect_package_manager(&declared), PackageManager::Yarn);
        let plain = project(&[("package.json", "{}"), ("x", "")]);
        assert_eq!(detect_package_manager(&plain), PackageManager::Npm);
        for d in [pnpm, declared, plain] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn suggestions_use_the_dev_script_and_port_flag() {
        let vite = project(&[("package.json", r#"{"scripts":{"dev":"vite"},"devDependencies":{"vite":"^7"}}"#)]);
        let s = suggest(&vite, 3001);
        assert_eq!(s.mode, AppMode::Command);
        assert_eq!(s.command, "npm run dev -- --host 127.0.0.1 --port {port}");
        assert!(s.serve && runnable(&vite));
        assert_eq!(framework(&vite), Some("Vite"));
        assert_eq!(crate::sites::language(&vite), crate::sites::Language::Node);

        // Laravel's Vite is an asset server: it runs, but the domain stays PHP's.
        let laravel = project(&[("package.json", r#"{"scripts":{"dev":"vite"},"devDependencies":{"vite":"^7"}}"#), ("artisan", "")]);
        let s = suggest(&laravel, 0);
        assert_eq!(crate::sites::language(&laravel), crate::sites::Language::Php);
        assert!(!s.serve);
        assert_eq!(s.command, "npm run dev");

        let library = project(&[("package.json", r#"{"scripts":{"test":"vitest"}}"#)]);
        assert!(!runnable(&library), "nothing starts a server");

        let next = project(&[("package.json", r#"{"scripts":{"dev":"next dev"},"dependencies":{"next":"16"}}"#), ("pnpm-lock.yaml", "")]);
        assert_eq!(suggest(&next, 3002).command, "pnpm run dev", "Next.js reads $PORT");

        let express = project(&[("package.json", r#"{"dependencies":{"express":"5"}}"#), ("server.js", "")]);
        let s = suggest(&express, 3003);
        assert_eq!(s.mode, AppMode::File);
        assert_eq!(s.file, "server.js");
        for d in [vite, next, express, laravel, library] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn npm_passes_script_arguments_after_a_separator() {
        let empty = OsString::new();
        assert_eq!(pm_commands(PackageManager::Npm, &empty).unwrap().args, "--");
        let pnpm = pm_commands(PackageManager::Pnpm, &empty).unwrap();
        assert_eq!(pnpm.args, "");
        assert_eq!(pnpm.run, "npx -y pnpm run", "pnpm runs through npx when it isn't installed");
        assert!(pm_commands(PackageManager::Bun, &empty).is_err());
    }
}
