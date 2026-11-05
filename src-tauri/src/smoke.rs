//! `lamprabit --smoke-test <folder> <runtime>…`: installs each runtime (with the versions
//! its needs call for) into a throwaway LampRabit folder, then puts every service through
//! the real supervisor: start, a healthy answer, a second start that must not spawn a
//! second process, stop, and a free port afterwards. One service is also killed to check
//! that "Restart after a crash" brings it back and that Stop then keeps it stopped. With PHP
//! and a web server, a project with extra domains and processes is checked through the
//! web server too. CI runs this on Windows, Linux and macOS (.github/workflows/smoke.yml).
//!
//! It never reads or writes the user's settings: the folder holds its own settings.toml,
//! and every port is moved up by `PORT_SHIFT` so nothing clashes with what the machine
//! runs and nothing needs administrator rights (ports below 1024, the hosts file).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::{
    AppState,
    catalog::{self, Action},
    config::Settings,
    error::{Error, Result},
    layout::Layout,
    runtimes::{self, Release, Runtime},
    services::{ServiceKind, ServiceState},
};

const PORT_SHIFT: u64 = 20_000;

pub struct Options {
    pub root: PathBuf,
    pub runtimes: Vec<Runtime>,
}

/// The smoke test's arguments, when the app was started with `--smoke-test`.
pub fn options() -> Option<std::result::Result<Options, String>> {
    let args: Vec<String> = std::env::args().collect();
    let at = args.iter().position(|a| a == "--smoke-test")?;
    let Some(root) = args.get(at + 1).filter(|a| !a.starts_with("--")) else {
        return Some(Err("usage: --smoke-test <empty folder> <runtime id>…".into()));
    };
    let mut runtimes = Vec::new();
    for id in &args[at + 2..] {
        match Runtime::from_id(id) {
            Some(r) => runtimes.push(r),
            None => return Some(Err(format!("unknown runtime \"{id}\""))),
        }
    }
    Some(Ok(Options { root: PathBuf::from(root), runtimes }))
}

/// Settings for the throwaway folder: no hosts file, no update checks, every port shifted,
/// and the web servers that are being tested enabled (the first one primary).
pub fn settings(options: &Options) -> Settings {
    let mut settings = Settings { root_dir: options.root.clone(), manage_hosts: false, check_updates: false, close_to_tray: false, ..Settings::default() };
    settings.auto_restart.clear();
    let mut ports = serde_json::to_value(&settings.ports).unwrap_or_default();
    if let Some(map) = ports.as_object_mut() {
        for value in map.values_mut() {
            if let Some(n) = value.as_u64().filter(|n| n + PORT_SHIFT < 65_000) {
                *value = (n + PORT_SHIFT).into();
            }
        }
    }
    if let Ok(shifted) = serde_json::from_value(ports) {
        settings.ports = shifted;
    }
    let servers: Vec<_> = options.runtimes.iter().filter_map(|r| ServiceKind::all().into_iter().find(|k| k.id() == r.id())).filter_map(|k| k.web_server()).collect();
    if !servers.is_empty() {
        settings.web_servers = servers;
    }
    settings
}

#[derive(Debug, Serialize)]
struct Step {
    name: String,
    /// "ok", "skipped" (not available on this platform) or "failed".
    result: &'static str,
    detail: String,
    seconds: f64,
}

struct Report {
    steps: Vec<Step>,
    path: PathBuf,
}

impl Report {
    fn add(&mut self, name: String, started: Instant, outcome: std::result::Result<String, Outcome>) {
        let (result, detail) = match outcome {
            Ok(detail) => ("ok", detail),
            Err(Outcome::Skipped(why)) => ("skipped", why),
            Err(Outcome::Failed(why)) => ("failed", why),
        };
        // stdout for the CI log as it goes; the JSON file for the summary.
        println!("{:<8} {name}: {detail}", result.to_uppercase());
        self.steps.push(Step { name, result, detail, seconds: started.elapsed().as_secs_f64() });
        let _ = std::fs::write(&self.path, serde_json::to_string_pretty(&self.steps).unwrap_or_default());
    }

    fn failed(&self) -> bool {
        self.steps.iter().any(|s| s.result == "failed")
    }
}

enum Outcome {
    Skipped(String),
    Failed(String),
}

impl Outcome {
    fn text(&self) -> String {
        match self {
            Outcome::Skipped(t) | Outcome::Failed(t) => t.clone(),
        }
    }
}

impl From<Error> for Outcome {
    fn from(e: Error) -> Self {
        Outcome::Failed(e.to_string())
    }
}

/// Runs everything and returns the process exit code: 0 when nothing failed.
pub async fn run(app: AppHandle, options: Options) -> i32 {
    let mut report = Report { steps: Vec::new(), path: options.root.join("smoke-report.json") };
    let mut installed = Vec::new();
    for &runtime in &options.runtimes {
        let started = Instant::now();
        let mut outcome = install(&app, runtime).await;
        // Downloads are large and CI networks drop connections now and then; one retry
        // keeps that from failing the run, while a real problem fails twice.
        if matches!(&outcome, Err(Outcome::Failed(e)) if e.contains("network error")) {
            println!("RETRY    install {}: {}", runtime.id(), outcome.as_ref().err().map(Outcome::text).unwrap_or_default());
            outcome = install(&app, runtime).await;
        }
        if outcome.is_ok() {
            installed.push(runtime);
        }
        report.add(format!("install {}", runtime.id()), started, outcome);
    }

    // A runtime that didn't install has already failed (or been skipped) above.
    let kinds: Vec<ServiceKind> = ServiceKind::all().into_iter().filter(|k| installed.iter().any(|r| r.id() == k.id())).collect();
    for &kind in &kinds {
        let started = Instant::now();
        let outcome = lifecycle(&app, kind).await;
        report.add(format!("service {}", kind.id()), started, outcome);
    }
    // The crash check on a quick, light service, when one was tested.
    let light = [ServiceKind::Redis, ServiceKind::Memcached, ServiceKind::Mailpit, ServiceKind::Meilisearch];
    if let Some(&kind) = light.iter().find(|k| kinds.contains(k)) {
        let started = Instant::now();
        let outcome = crash_restart(&app, kind).await;
        report.add(format!("restart after a crash ({})", kind.id()), started, outcome);
    }

    // A project's extra domains and processes, through the primary web server.
    let state = app.state::<AppState>();
    let primary = ServiceKind::for_server(state.snapshot().await.0.primary_server());
    if installed.contains(&Runtime::Php) && kinds.contains(&primary) {
        let started = Instant::now();
        let outcome = project(&app, primary).await;
        report.add("project domains and processes".into(), started, outcome);
        state.apps.stop_all().await;
    }

    app.state::<AppState>().supervisor.shutdown().await;
    let count = |result: &str| report.steps.iter().filter(|s| s.result == result).count();
    let (ok, skipped, failed) = (count("ok"), count("skipped"), count("failed"));
    println!("smoke test: {ok} ok, {skipped} skipped, {failed} failed ({})", report.path.display());
    if report.failed() { 1 } else { 0 }
}

/// Installs the release a user would get (LTS when there is one, else the newest) after
/// meeting its needs the way the install dialog does.
async fn install(app: &AppHandle, runtime: Runtime) -> std::result::Result<String, Outcome> {
    let state = app.state::<AppState>();
    let unavailable = catalog::catalog().runtimes.into_iter().find(|r| r.id == runtime.id()).and_then(|r| r.unavailable);
    if let Some(why) = unavailable {
        return Err(Outcome::Skipped(why.to_string()));
    }
    let releases = runtimes::available(&state.http, runtime, true).await?;
    // Python 3.13 for the same reason the requirements prefer it: pgAdmin's wheels.
    let preferred = |r: &&Release| if runtime == Runtime::Python { r.version.starts_with("3.13.") } else { r.label.as_deref() == Some("LTS") };
    let release = releases.iter().find(preferred).or(releases.first()).cloned().ok_or_else(|| Outcome::Skipped("no download for this platform".into()))?;
    let (settings, layout) = state.snapshot().await;
    let mut notes = Vec::new();
    for need in catalog::requirements(app, &state.http, &settings, &layout, runtime, Some(release.version.clone())).await? {
        match need.action {
            Action::Ready { version } => notes.push(format!("{} {version} in use", need.label)),
            Action::Activate { version } => {
                activate(app, need.runtime, &version).await?;
                notes.push(format!("switched to {} {version}", need.label));
            }
            Action::Install { release } => {
                install_release(app, &release).await?;
                notes.push(format!("installed {} {}", need.label, release.version));
            }
            Action::InstallNode { version } => {
                crate::node::install_version(app, &version).await?;
                crate::node::use_version(app, &version).await?;
                notes.push(format!("installed Node.js {version}"));
            }
            Action::UseNode { version } => {
                crate::node::use_version(app, &version).await?;
                notes.push(format!("using Node.js {version}"));
            }
            Action::Unavailable { reason } => return Err(Outcome::Failed(format!("{} {}: {reason}", need.label, need.range))),
        }
    }
    install_release(app, &release).await?;
    let mut detail = release.version.clone();
    if !notes.is_empty() {
        detail.push_str(&format!(" ({})", notes.join(", ")));
    }
    Ok(detail)
}

async fn install_release(app: &AppHandle, release: &Release) -> Result<()> {
    crate::install_runtime(app.clone(), app.state::<AppState>(), release.clone()).await?;
    activate(app, release.runtime.id(), &release.version).await
}

async fn activate(app: &AppHandle, runtime: &str, version: &str) -> Result<()> {
    let runtime = Runtime::from_id(runtime).ok_or_else(|| Error::msg(format!("unknown runtime {runtime}")))?;
    crate::set_active_version(app.clone(), app.state::<AppState>(), runtime, version.to_string()).await
}

async fn info(app: &AppHandle, kind: ServiceKind) -> (Settings, Layout, Option<crate::services::ServiceInfo>) {
    let state = app.state::<AppState>();
    let (settings, layout) = state.snapshot().await;
    let info = state.supervisor.list(&settings, &layout).await.into_iter().find(|s| s.kind == kind);
    (settings, layout, info)
}

/// Why a service isn't up: what the supervisor recorded (else `error`) and its log.
async fn why(app: &AppHandle, kind: ServiceKind, what: &str, error: Option<String>) -> Outcome {
    let (_, layout, info) = info(app, kind).await;
    let mut text = what.to_string();
    match (info.as_ref().and_then(|i| i.problem.as_ref()), error) {
        (Some(p), _) => text.push_str(&format!(": {} {}", p.title.trim_end_matches('.'), p.detail)),
        (None, Some(e)) => text.push_str(&format!(": {e}")),
        (None, None) => {}
    }
    let tail = crate::diagnose::tail(&layout.logs().join(format!("{}-output.log", kind.id())), 1500);
    if !tail.trim().is_empty() {
        text.push_str(&format!("\n--- {}-output.log ---\n{}", kind.id(), tail.trim()));
    }
    Outcome::Failed(text)
}

/// Waits for Running with a healthy probe; slow services get their whole start-up time.
async fn wait_healthy(app: &AppHandle, kind: ServiceKind) -> std::result::Result<u32, Outcome> {
    let state = app.state::<AppState>();
    let deadline = Instant::now() + Duration::from_secs(150);
    while Instant::now() < deadline {
        let (settings, _, info) = info(app, kind).await;
        match info.as_ref().map(|i| i.state) {
            Some(ServiceState::Running) => {
                if kind.probe_now(&settings, &state.http).await {
                    return Ok(info.and_then(|i| i.pid).unwrap_or_default());
                }
            }
            Some(ServiceState::Starting) => {}
            _ => return Err(why(app, kind, "stopped while starting", None).await),
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(why(app, kind, "didn't answer within 150 s", None).await)
}

async fn lifecycle(app: &AppHandle, kind: ServiceKind) -> std::result::Result<String, Outcome> {
    let state = app.state::<AppState>();
    if let Err(e) = crate::start_service(app.clone(), app.state::<AppState>(), kind).await {
        return Err(why(app, kind, "start failed", Some(e.to_string())).await);
    }
    let pid = wait_healthy(app, kind).await?;
    let took = format!("healthy (PID {pid})");

    // Start all, a profile and a second click all call start again: nothing new may spawn.
    let (settings, layout) = state.snapshot().await;
    state.supervisor.start(app, kind, &settings, &layout).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let (_, _, again) = info(app, kind).await;
    if again.as_ref().and_then(|i| i.pid) != Some(pid) {
        return Err(Outcome::Failed(format!("a second start replaced PID {pid} with {:?}", again.and_then(|i| i.pid))));
    }

    crate::stop_service(app.clone(), app.state::<AppState>(), kind).await?;
    if state.supervisor.is_running(kind).await {
        return Err(Outcome::Failed("still running after Stop".into()));
    }
    let port = kind.port_of(&settings);
    // Sockets can linger for a moment after the process is gone.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !crate::services::port_free(port) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    if !crate::services::port_free(port) {
        return Err(Outcome::Failed(format!("port {port} is still taken after Stop")));
    }
    Ok(format!("{took}, second start ignored, stopped, port {port} free"))
}

/// What a domain answers through the web server's HTTP port.
async fn fetch(app: &AppHandle, port: u16, host: &str) -> String {
    let client = &app.state::<AppState>().http;
    match client.get(format!("http://127.0.0.1:{port}/")).header("Host", host).send().await {
        Ok(r) => r.text().await.unwrap_or_default(),
        Err(e) => format!("(no answer: {e})"),
    }
}

/// A project with extra domains and processes: admin.<project> serves a folder, api.<project>
/// goes to a process (PHP's built-in server), a worker runs without a port, and a process
/// that always exits has to give up after a few restarts.
async fn project(app: &AppHandle, web: ServiceKind) -> std::result::Result<String, Outcome> {
    let state = app.state::<AppState>();
    let name = "smoke-site";
    let (settings, layout) = state.snapshot().await;
    let dir = layout.www().join(name);
    let write = |rel: &str, text: &str| -> Result<()> {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap_or(&dir))?;
        Ok(std::fs::write(path, text)?)
    };
    write("index.php", "<?php echo 'main-' . 'ok';")?;
    write("admin/index.html", "admin-ok")?;
    write("api/index.php", "<?php echo 'api-' . 'ok';")?;
    // Script files rather than `php -r "…"`: quoting through cmd.exe and the php shim
    // differs per OS, and a project's commands rarely look like that anyway.
    write("worker.php", "<?php sleep(3600);")?;
    write("flaky.php", "<?php exit(3);")?;
    let tld = &settings.tld;
    let api_port = crate::apps::free_port(&settings);
    let process = |name: &str, command: &str, port: Option<u16>, restart| crate::config::ProjectProcess {
        name: name.into(),
        command: command.into(),
        port,
        restart,
        with_app: false,
        ..Default::default()
    };
    let processes = vec![
        // Runs in api/ through its folder setting, not -t.
        crate::config::ProjectProcess { cwd: "api".into(), ..process("api", "php -S 127.0.0.1:{port}", Some(api_port), crate::config::RestartPolicy::OnFailure) },
        process("worker", "php worker.php", None, crate::config::RestartPolicy::OnFailure),
        process("flaky", "php flaky.php", None, crate::config::RestartPolicy::OnFailure),
    ];
    crate::set_site_processes(app.clone(), app.state::<AppState>(), name.into(), processes).await?;
    let domains = vec![
        crate::config::ExtraDomain { host: format!("admin.{name}.{tld}"), target: crate::config::DomainTarget::Files { root: "admin".into() } },
        crate::config::ExtraDomain { host: format!("api.{name}.{tld}"), target: crate::config::DomainTarget::Process { name: "api".into() } },
    ];
    // Checked like the app checks them: only subdomains of the project's own domain.
    for host in [format!("{name}.{tld}"), format!("other-thing.{tld}")] {
        let wrong = vec![crate::config::ExtraDomain { host: host.clone(), target: crate::config::DomainTarget::Main }];
        if crate::set_site_domains(app.clone(), app.state::<AppState>(), name.into(), wrong).await.is_ok() {
            return Err(Outcome::Failed(format!("{host} was accepted, but only subdomains of {name}.{tld} should be")));
        }
    }
    crate::set_site_domains(app.clone(), app.state::<AppState>(), name.into(), domains).await?;

    if !state.supervisor.is_running(web).await {
        crate::start_service(app.clone(), app.state::<AppState>(), web).await?;
    }
    wait_healthy(app, web).await?;
    let (settings, layout) = state.snapshot().await;
    for process in ["api", "worker", "flaky"] {
        state.apps.start_process(app, &settings, &layout, name, process).await?;
    }
    let http = settings.server_ports(settings.primary_server()).0;
    let checks = [(format!("{name}.{tld}"), "main-ok"), (format!("admin.{name}.{tld}"), "admin-ok"), (format!("api.{name}.{tld}"), "api-ok")];
    // The api process and the restarted server need a moment.
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut answers = Vec::new();
    loop {
        answers.clear();
        for (host, want) in &checks {
            let got = fetch(app, http, host).await;
            answers.push((host.clone(), *want, got));
        }
        if answers.iter().all(|(_, want, got)| got.contains(want)) || Instant::now() > deadline {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    if let Some((host, want, got)) = answers.iter().find(|(_, want, got)| !got.contains(want)) {
        return Err(Outcome::Failed(format!("{host} should answer \"{want}\", got: {}", got.chars().take(300).collect::<String>())));
    }

    // The main domain pointed by hand at the api process, then back to automatic.
    let main_host = format!("{name}.{tld}");
    let api_target = crate::config::DomainTarget::Process { name: "api".into() };
    crate::set_site_main(app.clone(), app.state::<AppState>(), name.into(), Some(api_target)).await?;
    let mut got = String::new();
    for _ in 0..20 {
        got = fetch(app, http, &main_host).await;
        if got.contains("api-ok") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    if !got.contains("api-ok") {
        return Err(Outcome::Failed(format!("{main_host}, pointed at the api process, answered: {}", got.chars().take(200).collect::<String>())));
    }
    crate::set_site_main(app.clone(), app.state::<AppState>(), name.into(), None).await?;
    for _ in 0..20 {
        got = fetch(app, http, &main_host).await;
        if got.contains("main-ok") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    if !got.contains("main-ok") {
        return Err(Outcome::Failed(format!("{main_host}, back on automatic, answered: {}", got.chars().take(200).collect::<String>())));
    }

    let worker = state.apps.status(&crate::apps::process_key(name, "worker")).await;
    if worker.state != "running" {
        return Err(Outcome::Failed(format!("the worker (no port) is {}, not running", worker.state)));
    }
    // flaky exits at once: restarted a few times (2 s apart), then left stopped.
    let key = crate::apps::process_key(name, "flaky");
    let deadline = Instant::now() + Duration::from_secs(40);
    let mut gave_up = false;
    while Instant::now() < deadline && !gave_up {
        tokio::time::sleep(Duration::from_secs(1)).await;
        gave_up = state.apps.logs(&layout, &key).await.iter().any(|l| l.contains("isn't restarted again"));
    }
    if !gave_up {
        return Err(Outcome::Failed("the process that always exits wasn't given up on after a few restarts".into()));
    }
    let restarts = state.apps.logs(&layout, &key).await.iter().filter(|l| l.contains("restarting it")).count();
    crate::processes_all(app.clone(), app.state::<AppState>(), name.into(), false).await?;
    Ok(format!("3 domains answer through {}, the main one follows a hand-picked process and back, worker running, flaky restarted {restarts} times then left stopped", web.label()))
}

/// Kills the service behind the supervisor's back and expects the restart Settings asks
/// for; then stops it and expects it to stay stopped.
async fn crash_restart(app: &AppHandle, kind: ServiceKind) -> std::result::Result<String, Outcome> {
    let state = app.state::<AppState>();
    state.update_settings(|s| {
        s.auto_restart.insert(kind.id().to_string());
    })
    .await?;
    crate::start_service(app.clone(), app.state::<AppState>(), kind).await?;
    let pid = wait_healthy(app, kind).await?;

    let mut system = sysinfo::System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[sysinfo::Pid::from_u32(pid)]), true);
    let killed = system.process(sysinfo::Pid::from_u32(pid)).is_some_and(|p| p.kill());
    if !killed {
        return Err(Outcome::Failed(format!("couldn't kill PID {pid} to simulate a crash")));
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut restarted = None;
    while Instant::now() < deadline && restarted.is_none() {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let (_, _, info) = info(app, kind).await;
        restarted = info.filter(|i| i.state == ServiceState::Running).and_then(|i| i.pid).filter(|&p| p != pid);
    }
    let Some(new_pid) = restarted else {
        return Err(why(app, kind, "wasn't restarted within 30 s of the crash", None).await);
    };
    wait_healthy(app, kind).await?;

    crate::stop_service(app.clone(), app.state::<AppState>(), kind).await?;
    // Longer than the restart loop's pause: a restart that ignored Stop would show by now.
    tokio::time::sleep(Duration::from_secs(4)).await;
    if state.supervisor.is_running(kind).await {
        return Err(Outcome::Failed("came back after Stop".into()));
    }
    Ok(format!("PID {pid} killed, restarted as {new_pid}, stayed stopped after Stop"))
}
