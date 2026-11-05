//! "New project": creates a folder in www/ by running a template's steps (templates.rs),
//! optionally with a database, and reports progress as it goes.

use std::{
    collections::{BTreeMap, HashMap},
    hash::{BuildHasher, Hasher},
    path::{Path, PathBuf},
    process::Stdio,
};

use serde::{Deserialize, Serialize};
use sha1::Digest as _;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::{
    config::{Database, Server, Settings},
    configs,
    error::{Error, Result},
    layout::{Layout, exe},
    procgroup,
    runtimes::{self, Runtime},
    shims,
    templates::{self, Action, Source, Template, Values},
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewProject {
    pub name: String,
    /// The template's id.
    pub template: String,
    /// PHP version for the site; None follows the active version.
    pub php: Option<String>,
    pub server: Option<Server>,
    pub ssl: bool,
    /// Database to create; None for no database.
    pub database: Option<String>,
    /// The server to create it on; the default database when absent.
    #[serde(default)]
    pub engine: Option<crate::templates::DbEngine>,
    /// Values for the template's variables, by id.
    #[serde(default)]
    pub variables: HashMap<String, String>,
    /// Optional steps, by id: run or not (absent: the step's default).
    #[serde(default)]
    pub optional: HashMap<String, bool>,
    /// The user saw and accepted the risky steps of a template that didn't ship with LampRabit.
    #[serde(default)]
    pub approve_risky: bool,
    /// Node templates: an NVM-installed version (None: NVM's default).
    #[serde(default)]
    pub node: Option<String>,
    /// Node templates: the package manager to create and run the project with.
    #[serde(default)]
    pub package_manager: Option<crate::config::PackageManager>,
}

/// What a finished template leaves for the app to save.
pub struct Built {
    /// The template's "what next".
    pub note: Option<String>,
    /// A Node project's run setup (without its port, which the caller picks).
    pub app: Option<crate::config::NodeApp>,
    /// The template's processes; port Some(0) means "pick a free one".
    pub processes: Vec<crate::config::ProjectProcess>,
    /// The template's subdomains, by their first label.
    pub domains: Vec<(String, crate::config::DomainTarget)>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub name: String,
    /// A step heading ("Downloading WordPress") or None for a line of tool output.
    pub step: Option<String>,
    pub line: Option<String>,
}

pub struct Ctx<'a> {
    pub app: &'a AppHandle,
    pub http: &'a reqwest::Client,
    pub settings: &'a Settings,
    pub layout: &'a Layout,
    pub name: &'a str,
}

impl Ctx<'_> {
    pub fn step(&self, step: impl Into<String>) {
        let _ = self.app.emit("project-progress", Progress { name: self.name.into(), step: Some(step.into()), line: None });
    }

    pub fn line(&self, line: impl Into<String>) {
        let _ = self.app.emit("project-progress", Progress { name: self.name.into(), step: None, line: Some(line.into()) });
    }

    fn dir(&self) -> PathBuf {
        self.layout.www().join(self.name)
    }
}

/// Folder names become DNS labels, so keep them to letters, digits, dashes and underscores.
/// Folder names Windows reserves for devices; creating them fails or misbehaves.
const RESERVED_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2",
    "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// MariaDB's own databases; a project must never write into these.
const SYSTEM_DATABASES: &[&str] = &["mysql", "information_schema", "performance_schema", "sys"];

pub fn validate(req: &NewProject, layout: &Layout) -> Result<()> {
    let name = req.name.trim();
    if name.len() > 63 {
        return Err(Error::msg("Keep the project name to 63 characters or fewer (it becomes the domain)."));
    }
    let ok = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !name.starts_with(['-', '_', '.']);
    if !ok {
        return Err(Error::msg("Use letters, numbers, dashes and underscores for the project name (it becomes the domain)."));
    }
    if RESERVED_NAMES.contains(&name.to_lowercase().as_str()) {
        return Err(Error::msg(format!("\"{name}\" is a reserved name on Windows. Choose another project name.")));
    }
    if layout.www().join(name).exists() {
        return Err(Error::msg(format!("www/{name} already exists.")));
    }
    if let Some(db) = &req.database {
        if db.is_empty() || db.len() > 64 || !db.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(Error::msg("Database names can use letters, numbers and underscores (up to 64)."));
        }
        if SYSTEM_DATABASES.contains(&db.to_lowercase().as_str()) {
            return Err(Error::msg(format!("{db} is one of MariaDB's own databases. Choose another name.")));
        }
    }
    Ok(())
}

/// The PHP binary for CLI work (Composer, artisan), with a php.ini to go with it.
fn php_cli(ctx: &Ctx, version: &str) -> Result<(PathBuf, PathBuf)> {
    let ini = configs::write_php(ctx.settings, ctx.layout, version, ctx.settings.ports.php_fpm)?;
    Ok((ctx.layout.bin("php", version).join(exe("php")), ini))
}

/// Runs a command, streaming its output as progress lines. On failure the error carries
/// the last few lines, which usually say what went wrong.
async fn run(ctx: &Ctx<'_>, program: &Path, args: &[String], cwd: &Path, extra_path: &[PathBuf]) -> Result<()> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("COMPOSER_HOME", ctx.layout.data("composer"))
        .env("COMPOSER_NO_INTERACTION", "1")
        // Large dependency trees (Magento) need more than PHP's default memory limit.
        .env("COMPOSER_MEMORY_LIMIT", "-1")
        // npx and `npm create` otherwise ask before downloading a package.
        .env("npm_config_yes", "true")
        .envs(crate::shims::tool_env(ctx.settings, ctx.layout));
    // PHP that a step starts by itself (Composer scripts, `php artisan serve`) gets no -c;
    // PHPRC points it at LampRabit's php.ini for the step's PHP version.
    let php_bins = ctx.layout.bin("php", "_");
    if let Some(version) = php_bins.parent().and_then(|root| extra_path.iter().find_map(|p| p.strip_prefix(root).ok()?.iter().next())) {
        cmd.env("PHPRC", ctx.layout.etc("php").join(version));
    }
    if !extra_path.is_empty() {
        let existing = std::env::var_os("PATH").unwrap_or_default();
        let paths = extra_path.iter().cloned().chain(std::env::split_paths(&existing));
        if let Ok(joined) = std::env::join_paths(paths) {
            cmd.env("PATH", joined);
        }
    }
    procgroup::prepare(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| Error::msg(format!("Couldn't run {}: {e}", program.display())))?;

    let tail = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let mut readers = Vec::new();
    for reader in [child.stdout.take().map(|r| Box::new(r) as Box<dyn tokio::io::AsyncRead + Unpin + Send>), child.stderr.take().map(|r| Box::new(r) as _)]
        .into_iter()
        .flatten()
    {
        let app = ctx.app.clone();
        let name = ctx.name.to_string();
        let tail = tail.clone();
        readers.push(tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = strip_ansi(&line);
                if line.trim().is_empty() {
                    continue;
                }
                let _ = app.emit("project-progress", Progress { name: name.clone(), step: None, line: Some(line.clone()) });
                let mut t = tail.lock().unwrap();
                t.push(line);
                if t.len() > 6 {
                    t.remove(0);
                }
            }
        }));
    }
    let status = child.wait().await?;
    for r in readers {
        let _ = r.await;
    }
    if status.success() {
        Ok(())
    } else {
        let tail = tail.lock().unwrap().join("\n");
        Err(Error::msg(format!("{} failed:\n{tail}", program.file_name().unwrap_or_default().to_string_lossy())))
    }
}

/// Drops terminal colour codes (`ESC[...m`), which Composer scripts print with --ansi.
pub fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            // Parameters, then one final letter.
            while let Some(n) = chars.next() {
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The active Composer, or the newest release installed the first time a template needs it
/// (the caller makes it the active version afterwards).
async fn ensure_composer(ctx: &Ctx<'_>) -> Result<PathBuf> {
    let installed = |v: &str| ctx.layout.bin("composer", v).join("composer.phar");
    if let Some(phar) = ctx.settings.versions.composer.as_deref().map(installed).filter(|p| p.exists()) {
        return Ok(phar);
    }
    ctx.step("Installing Composer");
    let release = runtimes::available(ctx.http, Runtime::Composer, false)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| Error::msg("Couldn't find a Composer download"))?;
    runtimes::install(ctx.app, ctx.http, ctx.layout, &release).await?;
    Ok(installed(&release.version))
}

/// Random letters, digits and punctuation, safe inside quoted PHP and .env strings.
pub fn random_string(len: usize) -> String {
    // RandomState is seeded from the OS RNG; good enough for local-only secrets.
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!#$%^&*()-_=+[]{}<>~";
    let mut out = String::with_capacity(len);
    while out.len() < len {
        let n = std::collections::hash_map::RandomState::new().build_hasher().finish();
        for i in 0..8 {
            if out.len() < len {
                out.push(CHARS[((n >> (i * 8)) & 0xff) as usize % CHARS.len()] as char);
            }
        }
    }
    out
}

/// The server a project's database goes on: the one asked for, else the default database.
pub fn engine(settings: &crate::config::Settings, req: &NewProject) -> Option<crate::templates::DbEngine> {
    use crate::templates::DbEngine;
    req.engine.or(match settings.database()? {
        Database::Mariadb => Some(DbEngine::Mariadb),
        Database::Mysql => Some(DbEngine::Mysql),
    })
}

/// PostgreSQL's psql, connected to LampRabit's server as its superuser.
fn psql(ctx: &Ctx<'_>) -> Result<(PathBuf, PathBuf, Vec<String>)> {
    let version = ctx.settings.versions.get("postgres").ok_or_else(|| Error::msg("Install PostgreSQL first (Services)."))?;
    let home = ctx.layout.bin("postgres", version);
    let args = vec!["-h".into(), "127.0.0.1".into(), "-p".into(), ctx.settings.ports.postgres.to_string(), "-U".into(), "postgres".into(), "-d".into(), "postgres".into()];
    Ok((home.join("bin").join(crate::layout::exe("psql")), home, args))
}

/// Creates the database on the project's server (which must be running).
pub async fn create_database(ctx: &Ctx<'_>, req: &NewProject, database: &str) -> Result<()> {
    use crate::templates::DbEngine;
    // SQLite's database is a file the framework creates itself (Laravel does on install).
    if engine(ctx.settings, req) == Some(DbEngine::Sqlite) {
        return Ok(());
    }
    if engine(ctx.settings, req) == Some(DbEngine::Postgres) {
        ctx.step(format!("Creating database {database} in PostgreSQL"));
        let (client, home, mut args) = psql(ctx)?;
        // template0 so the encoding can be set whatever template1 has.
        args.extend(["-v".into(), "ON_ERROR_STOP=1".into(), "-c".into(), format!("CREATE DATABASE \"{database}\" ENCODING 'UTF8' TEMPLATE template0")]);
        return run(ctx, &client, &args, &home, &[]).await.map_err(|e| {
            if e.to_string().contains("already exists") {
                Error::msg(format!("A database called {database} already exists. Choose another name, or leave the database option off and use the existing one yourself."))
            } else {
                e
            }
        });
    }
    let db = match req.engine {
        Some(DbEngine::Mysql) => Database::Mysql,
        Some(DbEngine::Mariadb) => Database::Mariadb,
        _ => ctx.settings.database().ok_or_else(|| Error::msg("Install MariaDB or MySQL first (Services)."))?,
    };
    ctx.step(format!("Creating database {database} in {}", db.label()));
    let home = configs::database_home(ctx.settings, ctx.layout, db)?;
    let client = configs::database_client(ctx.settings, ctx.layout, db)?;
    let args = vec![
        format!("--defaults-file={}", configs::write_client_config(ctx.settings, ctx.layout, db)?.display()),
        "-e".into(),
        // No IF NOT EXISTS: quietly sharing another project's database (and running its
        // migrations there) is worse than asking for a different name.
        format!("CREATE DATABASE `{database}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci"),
    ];
    run(ctx, &client, &args, &home, &[]).await.map_err(|e| {
        if e.to_string().contains("1007") {
            Error::msg(format!("A database called {database} already exists. Choose another name, or leave the database option off and use the existing one yourself."))
        } else {
            e
        }
    })
}

/// Sets `KEY=value` lines in a .env-style file, adding the keys that are missing. Matches
/// keys shipped commented out ("# DB_HOST=..." in Laravel, "# app.baseURL = ''" in
/// CodeIgniter) and keeps the file's ` = ` spacing.
pub(crate) fn set_env(path: &Path, values: &[(String, String)]) -> Result<()> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let spaced_file = lines.iter().any(|l| !l.trim_start().starts_with('#') && l.contains(" = "));
    for (key, value) in values {
        let found = lines.iter_mut().find(|l| {
            let bare = l.trim_start().trim_start_matches('#').trim_start();
            bare.split_once('=').is_some_and(|(k, _)| k.trim() == key)
        });
        match found {
            Some(line) => {
                let spaced = line.contains(" = ");
                *line = if spaced { format!("{key} = {value}") } else { format!("{key}={value}") };
            }
            None if spaced_file => lines.push(format!("{key} = {value}")),
            None => lines.push(format!("{key}={value}")),
        }
    }
    std::fs::write(path, lines.join("\n") + "\n")?;
    Ok(())
}

/// Values for a template's placeholders: the project, its database and Mailpit.
fn values(ctx: &Ctx<'_>, req: &NewProject, template: &Template, url: &str, php: Option<&str>, node: Option<(&str, &crate::apps::PmCommands)>) -> Values {
    use crate::templates::DbEngine;
    let engine = engine(ctx.settings, req).unwrap_or(DbEngine::Mariadb);
    let server = if engine == DbEngine::Mysql { Database::Mysql } else { Database::Mariadb };
    // Doctrine tells the two apart by serverVersion: "11.8.9-MariaDB" or "8.4.13".
    let version = ctx.settings.database_version(server).cloned().unwrap_or_else(|| "8.0".into());
    let server_version = if server == Database::Mariadb { format!("{version}-MariaDB") } else { version };
    let postgres_major = ctx.settings.versions.get("postgres").and_then(|v| v.split('.').next().map(str::to_string)).unwrap_or_else(|| "18".into());
    // Laravel's connection names; Doctrine's URL schemes and charsets.
    let (connection, port, server_version, scheme, charset, user) = match engine {
        DbEngine::Postgres => ("pgsql".to_string(), ctx.settings.ports.postgres, postgres_major, "postgresql", "utf8", "root"),
        DbEngine::Sqlite => ("sqlite".to_string(), 0, String::new(), "sqlite", "utf8", ""),
        _ => (server.id().to_string(), ctx.settings.database_port(server), server_version, "mysql", "utf8mb4", "root"),
    };
    let url = url.trim_end_matches('/').to_string();
    let domain = url.split("://").nth(1).unwrap_or(&url).to_string();
    let mut map = BTreeMap::from([
        ("project".to_string(), ctx.name.to_string()),
        ("domain".into(), domain),
        ("url".into(), url),
        ("www".into(), ctx.layout.www().to_string_lossy().into()),
        ("dir".into(), ctx.dir().to_string_lossy().into()),
        ("php_version".into(), php.unwrap_or_default().into()),
        // SQLite: the file's path, where Laravel looks by default.
        ("database".into(), if engine == DbEngine::Sqlite && req.database.is_some() {
            ctx.dir().join("database").join("database.sqlite").to_string_lossy().replace('\\', "/")
        } else {
            req.database.clone().unwrap_or_default()
        }),
        ("db.engine".into(), connection),
        ("db.host".into(), "127.0.0.1".into()),
        ("db.port".into(), port.to_string()),
        ("db.user".into(), user.into()),
        ("db.password".into(), String::new()),
        ("db.server_version".into(), server_version),
        ("db.scheme".into(), scheme.into()),
        ("db.charset".into(), charset.into()),
        // PDO's driver name (Drupal's too); empty without a database, so `if: db.pdo == pgsql`
        // only holds when there is one.
        ("db.pdo".into(), match (&req.database, engine) {
            (None, _) => "",
            (_, DbEngine::Postgres) => "pgsql",
            (_, DbEngine::Sqlite) => "sqlite",
            _ => "mysql",
        }.into()),
        // A project's virtual environment's Python, as each OS lays it out.
        ("venv.python".into(), if cfg!(windows) { r".venv\Scripts\python.exe".into() } else { ".venv/bin/python".into() }),
        // Maven's wrapper: a .cmd on Windows; elsewhere through sh, since zips drop the
        // executable bit.
        ("mvnw".into(), if cfg!(windows) { "mvnw.cmd".into() } else { "sh mvnw".into() }),
        ("java_major".into(), ctx.settings.versions.get("java").and_then(|v| v.split('.').next().map(str::to_string)).unwrap_or_else(|| "21".into())),
        ("mail.host".into(), "127.0.0.1".into()),
        ("mail.port".into(), ctx.settings.ports.mail_smtp.to_string()),
    ]);
    if let Some((version, pm)) = node {
        map.extend([
            ("node_version".to_string(), version.to_string()),
            ("pm".into(), pm.id.into()),
            ("pm.run".into(), pm.run.clone()),
            ("pm.args".into(), pm.args.into()),
            ("pm.exec".into(), pm.exec.clone()),
            ("pm.create".into(), pm.create.clone()),
            ("pm.install".into(), pm.install.clone()),
            ("pm.add".into(), pm.add.clone()),
        ]);
    }
    // Steps get what a variable renders (a flag, say); conditions what was picked ("raw.").
    for v in &template.variables {
        let value = req.variables.get(&v.id).cloned().or_else(|| v.default.clone()).unwrap_or_default();
        map.insert(format!("vars.{}", v.id), v.render(&value));
        map.insert(format!("raw.vars.{}", v.id), value);
    }
    // Variables shown only under a condition (Adobe's keys for Adobe's download) are empty
    // otherwise, so nothing leaks into steps that don't apply.
    let mut values = Values(map);
    for v in &template.variables {
        if let Some(when) = &v.when {
            if !values.holds(when).unwrap_or(true) {
                values.0.insert(format!("vars.{}", v.id), String::new());
                values.0.insert(format!("raw.vars.{}", v.id), String::new());
            }
        }
    }
    values
}

/// Whether a step runs: optional ones as chosen in the wizard, then its `if:`.
fn enabled(step: &templates::Step, index: usize, req: &NewProject, values: &Values) -> Result<bool> {
    if step.optional && !req.optional.get(&step.key(index)).copied().unwrap_or(step.default.unwrap_or(true)) {
        return Ok(false);
    }
    match &step.when {
        Some(condition) => values.holds(condition),
        None => Ok(true),
    }
}

/// "8.4.27" >= "8.2"
fn at_least(version: &str, minimum: &str) -> bool {
    let parts = |v: &str| v.split(|c: char| !c.is_ascii_digit()).filter_map(|p| p.parse::<u32>().ok()).collect::<Vec<_>>();
    parts(version) >= parts(minimum)
}

/// Whether a program answers `--version` on PATH (npm through nvm, Git).
async fn has_tool(program: &str) -> bool {
    let (shell, args) = shell(&format!("{program} --version"));
    let mut cmd = tokio::process::Command::new(shell);
    cmd.args(args).stdout(Stdio::null()).stderr(Stdio::null());
    procgroup::prepare(&mut cmd);
    cmd.status().await.is_ok_and(|s| s.success())
}

/// Everything a template needs, checked before anything is created: PHP version and
/// extensions, and the tools its enabled steps use.
async fn check_requirements(ctx: &Ctx<'_>, req: &NewProject, template: &Template, php: Option<&str>, steps: &[usize], path: &std::ffi::OsString) -> Result<()> {
    if let (Some(minimum), Some(version)) = (template.requires.php.as_deref().filter(|m| *m != "any"), php) {
        if !at_least(version, minimum) {
            return Err(Error::msg(format!(
                "{} needs PHP {minimum} or newer; this project would use {version}. Pick a newer PHP version (install one from Services → PHP).",
                template.name
            )));
        }
    }
    // The drivers for the database picked in the wizard, so templates that support
    // several engines don't have to list one engine's drivers for all of them.
    let mut extensions = template.requires.extensions.clone();
    if req.database.is_some() && template.engines.len() > 1 {
        use crate::templates::DbEngine;
        let drivers: &[&str] = match engine(ctx.settings, req) {
            Some(DbEngine::Postgres) => &["pdo_pgsql", "pgsql"],
            Some(DbEngine::Sqlite) => &["pdo_sqlite"],
            Some(DbEngine::Mariadb | DbEngine::Mysql) => &["pdo_mysql", "mysqli"],
            _ => &[],
        };
        extensions.extend(drivers.iter().map(|d| d.to_string()).filter(|d| !extensions.contains(d)).collect::<Vec<_>>());
    }
    if let (false, Some(version)) = (extensions.is_empty(), php) {
        let (php_bin, ini) = php_cli(ctx, version)?;
        let mut cmd = tokio::process::Command::new(&php_bin);
        cmd.arg("-c").arg(&ini).arg("-m");
        procgroup::prepare(&mut cmd);
        let out = cmd.output().await.map_err(|e| Error::msg(format!("Couldn't run PHP {version}: {e}")))?;
        let loaded: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().map(|l| l.trim().to_lowercase()).collect();
        let missing: Vec<&str> =
            extensions.iter().map(String::as_str).filter(|e| !loaded.iter().any(|l| *l == e.to_lowercase())).collect();
        if !missing.is_empty() {
            return Err(Error::msg(format!(
                "{} needs these PHP extensions, which PHP {version} doesn't load: {}. Turn them on in Services → PHP → Extensions.",
                template.name,
                missing.join(", ")
            )));
        }
    }
    // Python and Java projects run on LampRabit's own versions (they're on PATH as shims).
    for (id, label, minimum) in [
        ("python", "Python", &template.requires.python),
        ("java", "Java", &template.requires.java),
        ("go", "Go", &template.requires.go),
        ("rust", "Rust", &template.requires.rust),
    ] {
        let Some(minimum) = minimum else { continue };
        match ctx.settings.versions.get(id) {
            Some(have) if at_least(have, minimum) => {}
            Some(have) => {
                return Err(Error::msg(format!("{} needs {label} {minimum} or newer, and the active {label} is {have}. Switch to a newer one on the Services page.", template.name)));
            }
            None => return Err(Error::msg(format!("{} needs {label} {minimum} or newer. Install it on the Services page (Languages → {label}) first.", template.name))),
        }
    }
    let needs: std::collections::BTreeSet<&str> = steps.iter().flat_map(|&i| template.steps[i].needs.iter().map(String::as_str)).collect();
    for tool in needs {
        let (program, hint) = match tool {
            "node" => ("npm", "Install Node.js from Services → Node.js"),
            _ => ("git", "Install Git from git-scm.com"),
        };
        if !crate::apps::on_path(program, path) && !has_tool(program).await {
            return Err(Error::msg(format!("A step of this template needs {program}, which isn't on your PATH. {hint}, or untick that step.")));
        }
    }
    Ok(())
}

/// Downloads a file and checks it against the template's checksum, given inline or as the
/// URL of a published digest (WordPress publishes SHA-1).
async fn download_checked(ctx: &Ctx<'_>, d: &templates::Download, values: &Values) -> Result<Vec<u8>> {
    let err = |e: reqwest::Error| Error::msg(format!("Download failed: {e}"));
    let fetch_text = |url: String| async move { ctx.http.get(&url).send().await.and_then(|r| r.error_for_status()).map_err(err)?.text().await.map_err(err) };
    let url = values.render(&d.url);
    let expected = match (&d.sha256, &d.sha256_url, &d.sha1_url) {
        (Some(sum), _, _) => Some((sum.clone(), false)),
        (None, Some(u), _) => Some((fetch_text(values.render(u)).await?, false)),
        (None, None, Some(u)) => Some((fetch_text(values.render(u)).await?, true)),
        _ => None,
    };
    let bytes = ctx.http.get(&url).send().await.and_then(|r| r.error_for_status()).map_err(err)?.bytes().await.map_err(err)?;
    if let Some((expected, sha1_digest)) = expected {
        let expected = expected.split_whitespace().next().unwrap_or_default().to_ascii_lowercase();
        let actual: String = if sha1_digest {
            sha1::Sha1::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect()
        } else {
            sha2::Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect()
        };
        if actual != expected {
            return Err(Error::msg(format!("{url} failed its checksum check; the download may be corrupt.")));
        }
    }
    Ok(bytes.to_vec())
}

/// Unpacks a .zip or .tar.gz into the project folder, taking the contents of `strip` when
/// the archive wraps everything in one folder.
fn extract(ctx: &Ctx<'_>, bytes: Vec<u8>, url: &str, strip: Option<&str>, dir: &Path) -> Result<()> {
    let scratch = ctx.layout.tmp().join(format!("template-{}", ctx.name));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch)?;
    if url.ends_with(".tar.gz") || url.ends_with(".tgz") {
        tar::Archive::new(flate2::read::GzDecoder::new(std::io::Cursor::new(bytes))).unpack(&scratch)?;
    } else {
        zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .and_then(|mut z| z.extract(&scratch))
            .map_err(|e| Error::msg(format!("Couldn't unpack {url}: {e}")))?;
    }
    let source = match strip {
        Some(folder) => templates::inside(&scratch, folder)?,
        None => scratch.clone(),
    };
    if !source.is_dir() {
        return Err(Error::msg(format!("{url} has no folder \"{}\".", strip.unwrap_or_default())));
    }
    if dir.exists() {
        for entry in std::fs::read_dir(&source)?.flatten() {
            std::fs::rename(entry.path(), dir.join(entry.file_name()))?;
        }
    } else {
        std::fs::rename(&source, dir)?;
    }
    let _ = std::fs::remove_dir_all(&scratch);
    Ok(())
}

/// How a template's shell command runs: cmd on Windows, sh elsewhere.
pub fn shell(command: &str) -> (PathBuf, Vec<String>) {
    if cfg!(windows) {
        (PathBuf::from("cmd"), vec!["/C".into(), command.into()])
    } else {
        (PathBuf::from("sh"), vec!["-c".into(), command.into()])
    }
}

/// Replaces every occurrence of `find`, rendering `with` for each one, so `{{random:N}}`
/// differs every time (WordPress's eight keys and salts).
fn replace_each(text: &str, find: &str, with: &str, values: &Values) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(find) {
        out.push_str(&rest[..at]);
        out.push_str(&values.render(with));
        rest = &rest[at + find.len()..];
    }
    out.push_str(rest);
    out
}

/// Runs a template's steps for a new project and returns its "what next" note. On failure
/// the caller removes the folder.
pub async fn build(ctx: &Ctx<'_>, req: &NewProject, template: &Template, source: Source, url: &str) -> Result<Built> {
    let dir = ctx.dir();
    let php_version = match template.uses_php() {
        true => Some(req.php.clone().or_else(|| ctx.settings.versions.php.clone()).ok_or_else(|| Error::msg("Install PHP first (Services → PHP)."))?),
        false => None,
    };
    // Node: the chosen NVM version (or NVM's default) for every step that runs npm, node...
    // Steps that need Node in other templates (Laravel's npm build) use NVM's default.
    let wants_node = template.uses_node() || template.steps.iter().any(|s| s.needs.iter().any(|n| n == "node"));
    let node = match wants_node {
        true => match crate::node::for_project(req.node.as_deref(), &dir) {
            Ok(found) => Some(found),
            Err(e) if template.uses_node() => return Err(e),
            Err(_) => None,
        },
        false => None,
    };
    let node_bin: Option<PathBuf> = node.as_ref().map(|(_, bin)| bin.clone());
    let pm_commands = match (&node_bin, template.uses_node()) {
        (Some(bin), true) => {
            let path = crate::apps::node_path(bin, ctx.layout);
            Some(crate::apps::pm_commands(req.package_manager.unwrap_or_default(), &path)?)
        }
        _ => None,
    };
    let values = values(
        ctx,
        req,
        template,
        url,
        php_version.as_deref(),
        node.as_ref().zip(pm_commands.as_ref()).map(|((v, _), pm)| (v.as_str(), pm)),
    );
    for v in template.variables.iter().filter(|v| v.required) {
        let shown = v.when.as_deref().is_none_or(|w| values.holds(w).unwrap_or(true));
        if shown && values.get(&format!("vars.{}", v.id)).trim().is_empty() {
            return Err(Error::msg(format!("Fill in \"{}\" for this template.", v.label)));
        }
    }
    let mut steps = Vec::new();
    for (i, step) in template.steps.iter().enumerate() {
        if enabled(step, i, req, &values)? {
            steps.push(i);
        }
    }

    // A template that didn't ship with LampRabit runs risky commands only once approved.
    if source == Source::User && !req.approve_risky {
        let risky: Vec<String> = template
            .risks()
            .into_iter()
            .filter(|r| steps.iter().any(|&i| template.steps[i].key(i) == r.step))
            .map(|r| format!("{}: {}", r.command, r.reasons.join(", ")))
            .collect();
        if !risky.is_empty() {
            return Err(Error::msg(format!("This template has steps that need your approval first:\n{}", risky.join("\n"))));
        }
    }

    let php = match &php_version {
        Some(v) => Some((php_cli(ctx, v)?, ctx.layout.bin("php", v))),
        None => None,
    };
    // Steps find the project's Node and PHP first, then LampRabit's commands (composer, mysql…).
    let path: Vec<PathBuf> = node_bin.iter().cloned().chain(php.iter().map(|(_, bin)| bin.clone())).chain([shims::dir(ctx.layout)]).collect();
    let path_os = std::env::join_paths(path.iter().cloned().chain(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()))).unwrap_or_default();

    ctx.step("Checking requirements");
    check_requirements(ctx, req, template, php_version.as_deref(), &steps, &path_os).await?;
    let composer = match steps.iter().any(|&i| matches!(template.steps[i].action, Action::Composer(_))) {
        true => Some(ensure_composer(ctx).await?),
        false => None,
    };

    for &i in &steps {
        let step = &template.steps[i];
        if matches!(step.action, Action::CreateDatabase(_)) && req.database.is_none() {
            continue;
        }
        if !matches!(step.action, Action::CreateDatabase(_)) || step.name.is_some() {
            ctx.step(values.render(&step.heading()));
        }
        let cwd = match step.cwd.as_deref() {
            Some("www") => ctx.layout.www(),
            Some(sub) => templates::inside(&dir, &values.render(sub))?,
            None => dir.clone(),
        };
        let file = |p: &str| templates::inside(&dir, &values.render(p));
        let parent_of = |p: &Path| std::fs::create_dir_all(p.parent().unwrap_or(&dir));
        match &step.action {
            Action::Composer(args) | Action::Php(args) => {
                let ((php_bin, ini), _) = php.as_ref().ok_or_else(|| Error::msg(format!("{} runs PHP; add `requires: php:` to the template.", template.name)))?;
                let mut full = vec!["-c".to_string(), ini.to_string_lossy().into()];
                if let (Action::Composer(_), Some(phar)) = (&step.action, &composer) {
                    full.push(phar.to_string_lossy().into());
                }
                full.extend(args.iter().map(|a| values.render(a)));
                std::fs::create_dir_all(&cwd)?;
                run(ctx, php_bin, &full, &cwd, &path).await?;
            }
            Action::Run(command) => {
                let (program, args) = shell(&values.render(command));
                std::fs::create_dir_all(&cwd)?;
                run(ctx, &program, &args, &cwd, &path).await?;
            }
            Action::Download(d) => {
                let bytes = download_checked(ctx, d, &values).await?;
                let url = values.render(&d.url);
                if d.extract {
                    let strip = d.strip.as_deref().map(|s| values.render(s));
                    extract(ctx, bytes, &url, strip.as_deref(), &dir)?;
                } else {
                    let target = file(d.to.as_deref().unwrap_or_else(|| url.rsplit('/').next().unwrap_or("download")))?;
                    parent_of(&target)?;
                    std::fs::write(target, bytes)?;
                }
            }
            Action::Write(f) => {
                let target = file(&f.path)?;
                parent_of(&target)?;
                std::fs::write(target, values.render(&f.content))?;
            }
            Action::Append(f) => {
                use std::io::Write as _;
                let target = file(&f.path)?;
                parent_of(&target)?;
                std::fs::OpenOptions::new().create(true).append(true).open(target)?.write_all(values.render(&f.content).as_bytes())?;
            }
            Action::Copy(c) => {
                let (from, to) = (file(&c.from)?, file(&c.to)?);
                parent_of(&to)?;
                std::fs::copy(&from, &to).map_err(|e| Error::msg(format!("Couldn't copy {} to {}: {e}", c.from, c.to)))?;
            }
            Action::Env(e) => {
                let pairs: Vec<(String, String)> =
                    e.values.iter().filter_map(|(k, v)| Some((templates::scalar(k)?, values.render(&templates::scalar(v)?)))).collect();
                let target = file(&e.file)?;
                parent_of(&target)?;
                set_env(&target, &pairs)?;
            }
            Action::Replace(r) => {
                let target = file(&r.file)?;
                let mut text = std::fs::read_to_string(&target).map_err(|e| Error::msg(format!("Couldn't read {}: {e}", r.file)))?;
                for (find, with) in r.values.iter().filter_map(|(k, v)| Some((templates::scalar(k)?, templates::scalar(v)?))) {
                    let find = values.render(&find);
                    if text.contains(&find) {
                        text = replace_each(&text, &find, &with, &values);
                    } else {
                        // The upstream file changed; say so instead of failing the whole project.
                        ctx.line(format!("{} has no \"{find}\" any more; set that value by hand.", r.file));
                    }
                }
                std::fs::write(&target, text)?;
            }
            Action::CreateDatabase(_) => create_database(ctx, req, req.database.as_deref().unwrap_or_default()).await?,
            Action::GitInit(g) => {
                if !has_tool("git").await {
                    ctx.line("Git isn't installed; skipped the repository.");
                    continue;
                }
                std::fs::create_dir_all(&cwd)?;
                run(ctx, Path::new("git"), &["init".into(), "-q".into()], &cwd, &[]).await?;
                if let Some(message) = &g.commit {
                    let committed = match run(ctx, Path::new("git"), &["add".into(), "-A".into()], &cwd, &[]).await {
                        Ok(()) => run(ctx, Path::new("git"), &["commit".into(), "-q".into(), "-m".into(), values.render(message)], &cwd, &[]).await,
                        Err(e) => Err(e),
                    };
                    if committed.is_err() {
                        ctx.line("Skipped the first commit: tell Git who you are with git config --global user.name and user.email.");
                    }
                }
            }
        }
    }
    let app = template.serve.as_ref().map(|a| crate::config::NodeApp {
        mode: if a.file.trim().is_empty() { crate::config::AppMode::Command } else { crate::config::AppMode::File },
        command: values.render(&a.command),
        file: values.render(&a.file),
        watch: a.watch,
        port: 0,
        node: req.node.clone(),
        package_manager: req.package_manager,
        serve: a.mode == crate::config::ServeMode::Dynamic,
    });
    let holds = |when: &Option<String>| when.as_deref().is_none_or(|w| values.holds(w).unwrap_or(false));
    let processes = template
        .processes
        .iter()
        .filter(|p| holds(&p.when))
        .map(|p| crate::config::ProjectProcess {
            name: p.name.clone(),
            command: values.render(&p.command),
            cwd: values.render(&p.cwd),
            port: match p.port.as_deref() {
                Some("auto") => Some(0),
                Some(n) => n.parse().ok(),
                None => None,
            },
            env: p.env.iter().map(|(k, v)| (k.clone(), values.render(v))).collect(),
            restart: p.restart,
            with_app: p.autostart,
            ..Default::default()
        })
        .collect::<Vec<_>>();
    // A subdomain pointing at a process that was left out goes with it.
    let domains = template
        .domains
        .iter()
        .filter(|d| holds(&d.when) && d.process.as_ref().is_none_or(|p| processes.iter().any(|x| &x.name == p)))
        .map(|d| (d.name.clone(), d.target()))
        .collect();
    Ok(Built { note: template.done.as_deref().map(|d| values.render(d)), app, processes, domains })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_files_keep_their_style_and_commented_keys() {
        let path = std::env::temp_dir().join(format!("lamprabit-env-test-{}", std::process::id()));
        std::fs::write(&path, "APP_NAME=Laravel\n# DB_HOST=127.0.0.1\n# app.baseURL = ''\n").unwrap();
        let set = |k: &str, v: &str| (k.to_string(), v.to_string());
        set_env(&path, &[set("DB_HOST", "localhost"), set("app.baseURL", "'https://x.test/'"), set("NEW", "1")]).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "APP_NAME=Laravel\nDB_HOST=localhost\napp.baseURL = 'https://x.test/'\nNEW=1\n");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn colour_codes_are_removed() {
        assert_eq!(strip_ansi("\u{1b}[37;44m INFO \u{1b}[39;49m Done \u{1b}[22m."), " INFO  Done .");
    }

    #[test]
    fn versions_compare_by_number() {
        assert!(at_least("8.4.27", "8.2"));
        assert!(at_least("8.10.0", "8.9"));
        assert!(!at_least("8.1.34", "8.2"));
    }

    #[test]
    fn each_occurrence_gets_its_own_secret() {
        let values = Values(BTreeMap::new());
        let out = replace_each("A='x' B='x'", "x", "{{random:16}}", &values);
        let parts: Vec<&str> = out.split('\'').collect();
        assert_eq!(parts[1].len(), 16);
        assert_ne!(parts[1], parts[3]);
    }
}
