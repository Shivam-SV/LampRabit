mod apps;
mod backups;
mod catalog;
mod comfort;
mod commands;
mod config;
mod configs;
mod cron;
mod diagnose;
mod dumps;
mod error;
mod extensions;
mod facts;
mod langver;
mod layout;
mod logs;
mod manage;
mod node;
mod php;
mod ports;
mod procgroup;
mod projectfile;
mod projects;
mod relocate;
mod runtimes;
mod services;
mod shims;
mod sites;
mod smoke;
mod ssl;
mod templategen;
mod templates;
mod tunnels;
mod uninstall;

use std::{collections::HashSet, path::PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager, RunEvent, State};
use tokio::sync::{Mutex, RwLock};

use config::Settings;
use error::{Error, Result};
use layout::Layout;
use runtimes::{Release, Runtime};
use services::{ServiceInfo, ServiceKind, Supervisor};

struct AppState {
    settings: RwLock<Settings>,
    settings_path: PathBuf,
    supervisor: Supervisor,
    http: reqwest::Client,
    installing: Mutex<HashSet<(Runtime, String)>>,
    /// Cached result of the (slow) trust-store lookup; reset when trust changes.
    ca_trusted: Mutex<Option<bool>>,
    tunnels: tunnels::Tunnels,
    /// Held by anything that starts, stops or reconfigures services, so rapid clicks
    /// (Start all, Stop all, Start all…) run in order and the last one wins.
    ops: Mutex<()>,
    /// Process table for the dashboard's CPU and memory readings; kept between calls
    /// because CPU usage is measured from one refresh to the next.
    processes: std::sync::Mutex<sysinfo::System>,
    /// Node apps the user started from Sites (separate from services).
    apps: apps::Apps,
}

impl AppState {
    fn new(settings: Settings, settings_path: PathBuf) -> Self {
        AppState {
            settings: RwLock::new(settings),
            settings_path,
            supervisor: Supervisor::default(),
            http: runtimes::client(),
            installing: Mutex::new(HashSet::new()),
            ca_trusted: Mutex::new(None),
            tunnels: tunnels::Tunnels::default(),
            ops: Mutex::new(()),
            processes: std::sync::Mutex::new(sysinfo::System::new()),
            apps: apps::Apps::default(),
        }
    }

    async fn snapshot(&self) -> (Settings, Layout) {
        let settings = self.settings.read().await.clone();
        let layout = Layout::new(&settings.root_dir);
        (settings, layout)
    }

    async fn update_settings(&self, f: impl FnOnce(&mut Settings)) -> Result<Settings> {
        let mut settings = self.settings.write().await;
        f(&mut settings);
        settings.save(&self.settings_path)?;
        let _ = shims::write(&settings, &Layout::new(&settings.root_dir));
        Ok(settings.clone())
    }
}

/// Services that load the given runtime and must restart when it changes.
fn dependents(runtime: Runtime) -> &'static [ServiceKind] {
    match runtime {
        Runtime::Apache => &[ServiceKind::Apache],
        Runtime::Nginx => &[ServiceKind::Nginx],
        Runtime::Caddy => &[ServiceKind::Caddy],
        Runtime::Frankenphp => &[ServiceKind::Frankenphp],
        Runtime::Tomcat => &[ServiceKind::Tomcat],
        Runtime::Postgres => &[ServiceKind::Postgres],
        Runtime::Pgadmin => &[ServiceKind::Pgadmin],
        Runtime::Mongodb => &[ServiceKind::Mongodb],
        // The shell runs from the terminal; MongoDB uses it only to shut down.
        Runtime::Mongosh => &[],
        // Command-line tools and a desktop app: nothing runs them in the background.
        Runtime::Mongotools | Runtime::Compass | Runtime::Sqlite => &[],
        Runtime::Clickhouse => &[ServiceKind::Clickhouse],
        Runtime::Neo4j => &[ServiceKind::Neo4j],
        Runtime::Meilisearch => &[ServiceKind::Meilisearch],
        Runtime::Typesense => &[ServiceKind::Typesense],
        Runtime::Rustfs => &[ServiceKind::Rustfs],
        Runtime::Ollama => &[ServiceKind::Ollama],
        Runtime::Minio => &[ServiceKind::Minio],
        Runtime::Elasticsearch => &[ServiceKind::Elasticsearch],
        Runtime::Opensearch => &[ServiceKind::Opensearch],
        Runtime::N8n => &[ServiceKind::N8n],
        Runtime::Rabbitmq | Runtime::Erlang => &[ServiceKind::Rabbitmq],
        // Languages run from the terminal and projects' own processes.
        Runtime::Go | Runtime::Rust | Runtime::Maven | Runtime::Gradle => &[],
        // Projects run Python themselves; pgAdmin keeps the Python it was installed with.
        Runtime::Python => &[],
        Runtime::Phppgadmin => &ServiceKind::WEB,
        // PHP pools are owned by the web servers; restarting them restarts PHP.
        Runtime::Php => &ServiceKind::WEB,
        Runtime::Mariadb => &[ServiceKind::Mariadb],
        Runtime::Mysql => &[ServiceKind::Mysql],
        Runtime::Mailpit => &[ServiceKind::Mailpit],
        // Served by the primary web server through their own virtual hosts.
        Runtime::Adminer | Runtime::Phpmyadmin | Runtime::Phpredisadmin => &ServiceKind::WEB,
        // Tunnels start on demand; Composer runs from the terminal.
        Runtime::Ngrok | Runtime::Cloudflared | Runtime::Composer => &[],
        // Tomcat runs on the active Java.
        Runtime::Java => &[ServiceKind::Tomcat, ServiceKind::Neo4j],
        Runtime::Redis => &[ServiceKind::Redis],
        Runtime::Memcached => &[ServiceKind::Memcached],
    }
}

fn refresh_shims(app: &AppHandle, settings: &Settings) {
    if let Err(e) = shims::write(settings, &Layout::new(&settings.root_dir)) {
        logs::system(app, logs::Level::Warn, format!("Couldn't update command shims: {e}"));
    }
}

/// Drops "disabled" entries for extensions that are off by default anyway (older builds
/// stored them) and versions left with no choices.
fn tidy_extension_choices(settings: &mut Settings) -> bool {
    let before = settings.php_extensions.clone();
    for choices in settings.php_extensions.values_mut() {
        choices.disabled.retain(|name| extensions::DEFAULTS.contains(&name.as_str()));
    }
    settings.php_extensions.retain(|_, c| !c.enabled.is_empty() || !c.disabled.is_empty());
    settings.php_extensions != before
}

/// Drops active versions whose files are gone (e.g. deleted by hand).
fn reconcile_versions(settings: &mut Settings) {
    let layout = Layout::new(&settings.root_dir);
    for runtime in Runtime::ALL {
        let installed = runtimes::installed(&layout, runtime);
        if settings.versions.get(runtime.id()).is_none_or(|v| !installed.contains(v)) {
            settings.versions.set(runtime.id(), installed.into_iter().next());
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvPaths {
    root: PathBuf,
    www: PathBuf,
    logs: PathBuf,
    settings_file: PathBuf,
    /// "windows", "macos" or "linux", so the UI can hide OS-specific options.
    os: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstalledRuntime {
    runtime: Runtime,
    active: Option<String>,
    versions: Vec<String>,
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> Result<Settings> {
    Ok(state.settings.read().await.clone())
}

#[tauri::command]
async fn save_settings(app: AppHandle, state: State<'_, AppState>, mut settings: Settings) -> Result<Settings> {
    let old = state.settings.read().await.clone();
    check_values(&mut settings)?;
    check_ports(&settings)?;
    let layout = Layout::new(&settings.root_dir);
    check_root(&layout)?;
    // Running services belong to the old folder; stop them before switching.
    if state.settings.read().await.root_dir != settings.root_dir {
        let _op = state.ops.lock().await;
        let running: Vec<ServiceKind> = state.supervisor.pids().await.into_values().collect();
        if !running.is_empty() {
            logs::system(&app, logs::Level::Info, "Environment folder changed; stopping services from the old folder");
            for &kind in &running {
                state.supervisor.stop(&app, kind).await;
            }
            for kind in running {
                state.supervisor.wait_stopped(kind).await;
            }
        }
    }
    reconcile_versions(&mut settings);
    let was_on_path = state.settings.read().await.add_to_path;
    if settings.add_to_path != was_on_path {
        shims::set_on_path(&layout, settings.add_to_path)?;
        let what = if settings.add_to_path { "added to" } else { "removed from" };
        logs::system(&app, logs::Level::Info, format!("{} {what} your PATH", shims::dir(&layout).display()));
    }
    settings.save(&state.settings_path)?;
    *state.settings.write().await = settings.clone();
    refresh_shims(&app, &settings);
    *state.ca_trusted.lock().await = None;
    logs::set_system_file(&app, layout.logs().join("lampRabit.log"));
    logs::system(&app, logs::Level::Info, "Settings saved");
    for kind in ServiceKind::WEB {
        if !kind.enabled(&settings) && state.supervisor.is_running(kind).await {
            logs::system(&app, logs::Level::Info, format!("{} is switched off in Settings; stopping it", kind.label()));
            state.supervisor.stop(&app, kind).await;
        }
    }
    // Running services pick up what changed for them (ports, the primary server, the
    // domain suffix…) straight away; services in the old folder were stopped above.
    if old.root_dir == settings.root_dir {
        let old_layout = Layout::new(&old.root_dir);
        let mut restart = Vec::new();
        for kind in ServiceKind::all() {
            if kind.enabled(&settings)
                && state.supervisor.is_running(kind).await
                && service_fingerprint(kind, &old, &old_layout) != service_fingerprint(kind, &settings, &layout)
            {
                state.supervisor.stop(&app, kind).await;
                restart.push(kind);
            }
        }
        for &kind in &restart {
            state.supervisor.wait_stopped(kind).await;
        }
        for kind in restart {
            if let Err(e) = state.supervisor.start(&app, kind, &settings, &layout).await {
                logs::system(&app, logs::Level::Error, format!("{} didn't restart with the new settings: {e}", kind.label()));
            }
        }
    }
    Ok(settings)
}

/// What a settings change can alter in a running service: its ports and, for web servers,
/// everything their configs are built from. A different fingerprint means a restart.
fn service_fingerprint(kind: ServiceKind, settings: &Settings, layout: &Layout) -> String {
    let ports = services::required_ports(kind, settings, layout);
    if kind.web_server().is_some() {
        format!(
            "{ports:?} {:?} {:?} {} {} {} {}",
            settings.web_servers, settings.ports, settings.tld, settings.https_by_default, settings.php_workers, settings.route_mail
        )
    } else if matches!(kind, ServiceKind::Rustfs | ServiceKind::Minio) {
        format!("{ports:?} {:?}", settings.storage_dir(layout, kind.id()))
    } else {
        format!("{ports:?}")
    }
}

/// Domain endings that exist on the internet: using one would send real websites
/// (google.com, …) to this machine. .dev and .app also force HTTPS in browsers.
const PUBLIC_TLDS: &[&str] = &[
    "com", "net", "org", "io", "co", "dev", "app", "ai", "me", "info", "biz", "us", "uk", "in", "de", "fr", "eu",
    "xyz", "site", "online", "tech", "store", "cloud", "page", "gov", "edu", "mil",
];

/// Tidies and checks values the UI can't fully constrain. Rejects rather than guesses
/// where a guess could surprise someone.
fn check_values(settings: &mut Settings) -> Result<()> {
    let tld = settings.tld.trim().trim_matches('.').to_lowercase();
    let valid_label = |l: &str| !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-') && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if tld.is_empty() || !tld.split('.').all(valid_label) {
        return Err(Error::msg("The domain suffix can use letters, numbers and dashes, for example \"test\" or \"localhost\"."));
    }
    let last = tld.rsplit('.').next().unwrap_or_default();
    if PUBLIC_TLDS.contains(&last) {
        return Err(Error::msg(format!(
            ".{tld} is a real domain ending on the internet, so it would hide real websites. Use \"test\" or \"localhost\"."
        )));
    }
    settings.tld = tld;
    settings.normalize();
    if !(1..=32).contains(&settings.php_workers) {
        return Err(Error::msg("PHP workers must be between 1 and 32."));
    }
    let p = &settings.ports;
    for (port, name) in [
        (p.http, "HTTP"), (p.https, "HTTPS"), (p.nginx_http, "Nginx HTTP"), (p.nginx_https, "Nginx HTTPS"),
        (p.apache_http, "Apache HTTP"), (p.apache_https, "Apache HTTPS"), (p.caddy_http, "Caddy HTTP"), (p.caddy_https, "Caddy HTTPS"),
        (p.caddy_admin, "Caddy admin"), (p.frankenphp_http, "FrankenPHP HTTP"), (p.frankenphp_https, "FrankenPHP HTTPS"),
        (p.frankenphp_admin, "FrankenPHP admin"), (p.tomcat_http, "Tomcat HTTP"), (p.tomcat_shutdown, "Tomcat shutdown"), (p.mysql, "MariaDB"), (p.mail_smtp, "Mailpit SMTP"), (p.mail_ui, "Mailpit inbox"), (p.redis, "Redis"),
        (p.memcached, "Memcached"), (p.php_fpm, "PHP FastCGI"), (p.postgres, "PostgreSQL"), (p.pgadmin, "pgAdmin"), (p.mongodb, "MongoDB"), (p.clickhouse_http, "ClickHouse HTTP"), (p.clickhouse_tcp, "ClickHouse native"), (p.neo4j_http, "Neo4j Browser"), (p.neo4j_bolt, "Neo4j Bolt"), (p.meilisearch, "Meilisearch"), (p.typesense, "Typesense"), (p.rustfs, "RustFS"), (p.rustfs_console, "RustFS console"), (p.ollama, "Ollama"), (p.minio, "MinIO"), (p.minio_console, "MinIO console"), (p.elasticsearch, "Elasticsearch"), (p.elasticsearch_transport, "Elasticsearch transport"), (p.opensearch, "OpenSearch"), (p.opensearch_transport, "OpenSearch transport"), (p.n8n, "n8n"), (p.rabbitmq, "RabbitMQ"), (p.rabbitmq_management, "RabbitMQ console"),
    ] {
        if port == 0 {
            return Err(Error::msg(format!("Choose a port for {name} (1–65535).")));
        }
    }
    if p.php_fpm > 65535 - 99 {
        return Err(Error::msg("PHP FastCGI needs 100 ports from its starting port; choose a starting port below 65437."));
    }
    Ok(())
}

/// The environment folder must be creatable, writable and outside system folders.
fn check_root(layout: &Layout) -> Result<()> {
    let root = layout.root();
    if !root.is_absolute() {
        return Err(Error::msg("Choose a full path for the environment folder, for example C:\\lampRabit."));
    }
    #[cfg(windows)]
    {
        let lower = root.to_string_lossy().to_lowercase();
        let system: Vec<String> = ["SystemRoot", "ProgramFiles", "ProgramFiles(x86)", "ProgramData"]
            .iter()
            .filter_map(|v| std::env::var(v).ok())
            .map(|v| v.to_lowercase())
            .collect();
        if system.iter().any(|s| lower == *s || lower.starts_with(&format!("{s}\\"))) {
            return Err(Error::msg(
                "The environment folder can't be inside Windows or Program Files: those need administrator rights. Use something like C:\\lampRabit.",
            ));
        }
    }
    layout.ensure().map_err(|e| Error::msg(format!("Couldn't create {}: {e}", root.display())))?;
    // Some locations accept folder creation but not writes (or redirect them); check for real.
    let probe = layout.tmp().join(".lamprabit-write-test");
    std::fs::write(&probe, b"ok")
        .and_then(|_| std::fs::read(&probe))
        .map_err(|e| Error::msg(format!("LampRabit can't write to {}: {e}", root.display())))?;
    let _ = std::fs::remove_file(probe);
    Ok(())
}

/// Rejects settings where two services would listen on the same port.
fn check_ports(settings: &Settings) -> Result<()> {
    use config::Server;
    let p = &settings.ports;
    let mut used: Vec<(u16, String)> = vec![
        (p.mysql, "MariaDB".into()),
        (p.mail_smtp, "Mailpit SMTP".into()),
        (p.mail_ui, "Mailpit inbox".into()),
        (p.redis, "Redis".into()),
        (p.postgres, "PostgreSQL".into()),
        (p.pgadmin, "pgAdmin".into()),
        (p.mongodb, "MongoDB".into()),
        (p.clickhouse_http, "ClickHouse HTTP".into()),
        (p.clickhouse_tcp, "ClickHouse native".into()),
        (p.neo4j_http, "Neo4j Browser".into()),
        (p.neo4j_bolt, "Neo4j Bolt".into()),
        (p.meilisearch, "Meilisearch".into()),
        (p.typesense, "Typesense".into()),
        (p.rustfs, "RustFS".into()),
        (p.rustfs_console, "RustFS console".into()),
        (p.ollama, "Ollama".into()),
        (p.minio, "MinIO".into()),
        (p.minio_console, "MinIO console".into()),
        (p.elasticsearch, "Elasticsearch".into()),
        (p.elasticsearch_transport, "Elasticsearch transport".into()),
        (p.opensearch, "OpenSearch".into()),
        (p.opensearch_transport, "OpenSearch transport".into()),
        (p.n8n, "n8n".into()),
        (p.rabbitmq, "RabbitMQ".into()),
        (p.rabbitmq_management, "RabbitMQ console".into()),
        (p.memcached, "Memcached".into()),
    ];
    for server in Server::ALL {
        if settings.server_enabled(server) {
            let (http, https) = settings.server_ports(server);
            let name = server.label();
            used.push((http, format!("{name} HTTP")));
            used.push((https, format!("{name} HTTPS")));
        }
    }
    if settings.server_enabled(Server::Caddy) {
        used.push((p.caddy_admin, "Caddy admin".into()));
    }
    if settings.server_enabled(Server::Frankenphp) {
        used.push((p.frankenphp_admin, "FrankenPHP admin".into()));
    }
    if settings.versions.get("tomcat").is_some() {
        used.push((p.tomcat_http, "Tomcat HTTP".into()));
        used.push((p.tomcat_shutdown, "Tomcat shutdown".into()));
    }
    let php_block = p.php_fpm..p.php_fpm.saturating_add(100);
    for (i, (port, a)) in used.iter().enumerate() {
        if let Some((_, b)) = used[i + 1..].iter().find(|(q, _)| q == port) {
            return Err(Error::msg(format!("{a} and {b} are both set to port {port}. Give each its own port.")));
        }
        if php_block.contains(port) {
            return Err(Error::msg(format!(
                "{a} (port {port}) overlaps the PHP FastCGI ports ({}-{}). Move one of them.",
                php_block.start,
                php_block.end - 1
            )));
        }
    }
    Ok(())
}

/// The service of a database engine, which has to be running for its tools to connect.
async fn running_database(state: &AppState, engine: templates::DbEngine) -> Result<()> {
    let kind = match engine {
        templates::DbEngine::Mariadb => ServiceKind::Mariadb,
        templates::DbEngine::Mysql => ServiceKind::Mysql,
        templates::DbEngine::Postgres => ServiceKind::Postgres,
        templates::DbEngine::Mongodb => ServiceKind::Mongodb,
        // Files in the projects: nothing has to run.
        templates::DbEngine::Sqlite => return Ok(()),
    };
    if state.supervisor.is_running(kind).await {
        Ok(())
    } else {
        Err(Error::msg(format!("Start {} first: backups are made and restored through the running server.", engine.label())))
    }
}

#[tauri::command]
async fn list_databases(state: State<'_, AppState>, engine: templates::DbEngine) -> Result<Vec<String>> {
    running_database(&state, engine).await?;
    let (settings, layout) = state.snapshot().await;
    backups::databases(&settings, &layout, engine).await
}

#[tauri::command]
async fn list_backups(state: State<'_, AppState>) -> Result<Vec<backups::Backup>> {
    Ok(backups::list(&state.snapshot().await.1))
}

#[tauri::command]
async fn backup_database(app: AppHandle, state: State<'_, AppState>, engine: templates::DbEngine, database: String) -> Result<backups::Backup> {
    running_database(&state, engine).await?;
    let (settings, layout) = state.snapshot().await;
    let backup = backups::create(&settings, &layout, engine, &database).await?;
    logs::system(&app, logs::Level::Info, format!("Backed up {} database {database} to {}", engine.label(), backup.path.display()));
    Ok(backup)
}

#[tauri::command]
async fn restore_backup(app: AppHandle, state: State<'_, AppState>, engine: templates::DbEngine, path: PathBuf, database: String) -> Result<()> {
    running_database(&state, engine).await?;
    let (settings, layout) = state.snapshot().await;
    logs::system(&app, logs::Level::Info, format!("Restoring {} into {} database {database}", path.display(), engine.label()));
    backups::restore(&settings, &layout, engine, &path, &database).await?;
    logs::system(&app, logs::Level::Info, format!("Restored {database}"));
    Ok(())
}

/// Set on the first run, until the LampRabit folder's place is confirmed.
struct FirstRun(std::sync::atomic::AtomicBool);

/// Whether this is the first run, when the app asks where the LampRabit folder goes.
#[tauri::command]
fn first_run(flag: State<'_, FirstRun>) -> bool {
    flag.0.load(std::sync::atomic::Ordering::Relaxed)
}

/// The LampRabit folder's place is settled: the settings are written, so later runs
/// don't ask again.
#[tauri::command]
async fn finish_first_run(app: AppHandle, state: State<'_, AppState>, flag: State<'_, FirstRun>) -> Result<()> {
    flag.0.store(false, std::sync::atomic::Ordering::Relaxed);
    let settings = state.update_settings(|_| {}).await?;
    logs::system(&app, logs::Level::Info, format!("LampRabit's folder is {}", settings.root_dir.display()));
    Ok(())
}

/// Whether a folder can hold the LampRabit folder, and what moving there involves.
#[tauri::command]
async fn check_move_target(state: State<'_, AppState>, target: PathBuf) -> Result<relocate::MoveCheck> {
    let root = state.snapshot().await.1.root().to_path_buf();
    Ok(tauri::async_runtime::spawn_blocking(move || relocate::check(&root, &target)).await.map_err(|e| Error::msg(e.to_string()))?)
}

/// Programs other than LampRabit that keep the folder busy.
#[tauri::command]
async fn environment_in_use(state: State<'_, AppState>) -> Result<Vec<relocate::Holder>> {
    let root = state.snapshot().await.1.root().to_path_buf();
    Ok(relocate::holders(&root))
}

#[tauri::command]
async fn stop_environment_holder(app: AppHandle, state: State<'_, AppState>, pid: u32) -> Result<()> {
    let root = state.snapshot().await.1.root().to_path_buf();
    relocate::stop_holder(&root, pid)?;
    logs::system(&app, logs::Level::Info, format!("Stopped process {pid}, which was using the LampRabit folder"));
    Ok(())
}

/// Moves the LampRabit folder to `target`: stops everything that runs from it, moves it
/// (all or nothing), repairs what records absolute paths, and switches the settings,
/// shims and PATH over. Services aren't restarted; the user starts what they need.
#[tauri::command]
async fn move_environment(app: AppHandle, state: State<'_, AppState>, target: PathBuf) -> Result<Settings> {
    let _op = state.ops.lock().await;
    let (settings, layout) = state.snapshot().await;
    let old = layout.root().to_path_buf();
    let check = {
        let (old, target) = (old.clone(), target.clone());
        tauri::async_runtime::spawn_blocking(move || relocate::check(&old, &target)).await.map_err(|e| Error::msg(e.to_string()))?
    };
    if !check.problems.is_empty() {
        return Err(Error::msg(check.problems.join(" ")));
    }
    logs::system(&app, logs::Level::Info, format!("Moving the LampRabit folder from {} to {}", old.display(), target.display()));
    // Everything LampRabit runs from the folder stops first.
    let running: Vec<ServiceKind> = state.supervisor.pids().await.into_values().collect();
    for &kind in &running {
        state.supervisor.stop(&app, kind).await;
    }
    for kind in running {
        state.supervisor.wait_stopped(kind).await;
    }
    state.apps.stop_all().await;
    state.tunnels.stop_all().await;
    state.supervisor.php.stop_all(&app).await;
    let busy = relocate::holders(&old);
    if !busy.is_empty() {
        let names: Vec<String> = busy.iter().map(|h| format!("{} (PID {})", h.name, h.pid)).collect();
        return Err(Error::msg(format!("These programs still use the folder: {}. Stop them and try again.", names.join(", "))));
    }
    if settings.add_to_path {
        let _ = shims::set_on_path(&layout, false);
    }
    let (from, to, same) = (old.clone(), target.clone(), check.same_drive);
    let app2 = app.clone();
    let moved = tauri::async_runtime::spawn_blocking(move || relocate::move_root(&app2, &from, &to, same)).await.map_err(|e| Error::msg(e.to_string()))?;
    if let Err(e) = moved {
        if settings.add_to_path {
            let _ = shims::set_on_path(&layout, true);
        }
        logs::system(&app, logs::Level::Error, e.to_string());
        return Err(e);
    }
    relocate::repair_venvs(&old, &target);
    let settings = state.update_settings(|s| s.root_dir = target.clone()).await?;
    let layout = Layout::new(&settings.root_dir);
    if settings.add_to_path {
        if let Err(e) = shims::set_on_path(&layout, true) {
            logs::system(&app, logs::Level::Warn, format!("Couldn't put the new shims folder on PATH: {e}"));
        }
    }
    *state.ca_trusted.lock().await = None;
    logs::set_system_file(&app, layout.logs().join("lampRabit.log"));
    logs::system(&app, logs::Level::Info, format!("The LampRabit folder is now {}", target.display()));
    Ok(settings)
}

/// Sets the version a Python or Java project uses, by writing its version file (None
/// removes it, back to the active version); its shims follow straight away.
#[tauri::command]
async fn set_language_version(app: AppHandle, state: State<'_, AppState>, name: String, version: Option<String>) -> Result<()> {
    let (settings, layout) = state.snapshot().await;
    let dir = layout.www().join(&name);
    let runtime = match sites::language(&dir) {
        sites::Language::Python => "python",
        sites::Language::Java => "java",
        sites::Language::Go => "go",
        sites::Language::Rust => "rust",
        _ => return Err(Error::msg("This project's language has no version to choose.")),
    };
    langver::set(&dir, runtime, version.as_deref())?;
    refresh_shims(&app, &settings);
    Ok(())
}

/// Where an Ollama installed outside LampRabit is, if there is one.
#[tauri::command]
fn ollama_elsewhere() -> Option<PathBuf> {
    runtimes::ollama_elsewhere()
}

/// Opens MongoDB Compass connected to LampRabit's MongoDB.
#[tauri::command]
async fn open_compass(state: State<'_, AppState>) -> Result<()> {
    let (settings, layout) = state.snapshot().await;
    let version = settings.versions.get("compass").ok_or_else(|| Error::msg("Install MongoDB Compass first (Services)."))?;
    let home = layout.bin("compass", version);
    let program = runtimes::compass_program(&home).ok_or_else(|| Error::msg("Compass's install looks incomplete. Remove it on the Services page and install it again."))?;
    let uri = format!("mongodb://127.0.0.1:{}", settings.ports.mongodb);
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg("-a").arg(&program).arg("--args").arg(&uri);
        c
    } else {
        let mut c = std::process::Command::new(&program);
        c.arg(&uri);
        c
    };
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    cmd.spawn().map_err(|e| Error::msg(format!("Couldn't open Compass: {e}")))?;
    Ok(())
}

#[tauri::command]
async fn delete_backup(state: State<'_, AppState>, path: PathBuf) -> Result<()> {
    backups::delete(&state.snapshot().await.1, &path)
}

/// Makes a new project's domain work right away: its certificate is issued, the local CA
/// trusted and the hosts file updated now, so the admin prompts come while the user is
/// watching the wizard instead of later, when a web server happens to start. A cancelled
/// prompt doesn't fail the project; the Sites page offers the same fixes.
async fn prepare_new_site(app: &AppHandle, state: &AppState, name: &str) {
    let (settings, layout) = state.snapshot().await;
    let served = sites::served(&settings, &layout);
    if let Some(site) = served.iter().find(|s| s.name == name && s.ssl) {
        match ssl::ensure_ca(&layout).and_then(|(ca, _)| ssl::ensure_site_cert(&layout, &ca, &site.domain)) {
            Ok(_) if !ssl::ca_trusted(&layout).await => {
                *state.ca_trusted.lock().await = None;
                if let Err(e) = ssl::trust_ca(&layout).await {
                    logs::system(app, logs::Level::Warn, format!("{e} Browsers will warn about {} until LampRabit's certificate authority is trusted (Sites page).", site.domain));
                }
            }
            Ok(_) => {}
            Err(e) => logs::system(app, logs::Level::Warn, format!("Couldn't create a certificate for {}: {e}", site.domain)),
        }
    }
    if settings.manage_hosts && sites::needs_hosts_file(&settings) && !sites::hosts_in_sync(&served) {
        match sites::update_hosts(&layout, &served).await {
            Ok(()) => logs::system(app, logs::Level::Info, "Hosts file updated"),
            Err(e) => logs::system(app, logs::Level::Warn, format!("{e} The new domain may not resolve until the hosts file is updated (Sites page).")),
        }
    }
}

/// Restarts whichever web servers are running, so they (and their PHP pools) pick up
/// changed sites or PHP versions together.
async fn restart_web_servers(app: &AppHandle, state: &AppState) -> Result<()> {
    let mut running = Vec::new();
    for kind in ServiceKind::WEB {
        if state.supervisor.is_running(kind).await {
            state.supervisor.stop(app, kind).await;
            running.push(kind);
        }
    }
    // Tomcat reads its hosts only at start; restart it when the Java apps changed.
    {
        let (settings, layout) = state.snapshot().await;
        if state.supervisor.is_running(ServiceKind::Tomcat).await
            && configs::tomcat_stale(&settings, &layout, &sites::tomcat_apps(&settings, &layout))
        {
            state.supervisor.stop(app, ServiceKind::Tomcat).await;
            running.push(ServiceKind::Tomcat);
        }
    }
    for &kind in &running {
        state.supervisor.wait_stopped(kind).await;
    }
    let (settings, layout) = state.snapshot().await;
    for kind in running {
        state.supervisor.start(app, kind, &settings, &layout).await?;
    }
    Ok(())
}

#[tauri::command]
async fn env_paths(state: State<'_, AppState>) -> Result<EnvPaths> {
    let (_, layout) = state.snapshot().await;
    Ok(EnvPaths {
        root: layout.root().to_path_buf(),
        www: layout.www(),
        logs: layout.logs(),
        settings_file: state.settings_path.clone(),
        os: std::env::consts::OS,
    })
}

#[tauri::command]
async fn list_services(state: State<'_, AppState>) -> Result<Vec<ServiceInfo>> {
    let (settings, layout) = state.snapshot().await;
    Ok(state.supervisor.list(&settings, &layout).await)
}

#[tauri::command]
async fn start_service(app: AppHandle, state: State<'_, AppState>, kind: ServiceKind) -> Result<()> {
    let _op = state.ops.lock().await;
    let (settings, layout) = state.snapshot().await;
    state.supervisor.start(&app, kind, &settings, &layout).await?;
    serve_tool_domain(&app, &state, kind).await;
    Ok(())
}

/// Services with a web UI of their own at a .test domain (n8n.test, pgadmin.test…), which
/// the primary web server proxies.
fn tool_domain_service(kind: ServiceKind) -> bool {
    matches!(kind, ServiceKind::N8n | ServiceKind::Pgadmin | ServiceKind::Neo4j | ServiceKind::Mailpit)
}

/// Makes a just-started service's .test domain answer: the primary web server is started
/// when it isn't running (which also brings the hosts file up to date), or reloaded when
/// its config doesn't have the domain yet. A failure is logged; the service itself runs.
async fn serve_tool_domain(app: &AppHandle, state: &AppState, kind: ServiceKind) {
    if !tool_domain_service(kind) {
        return;
    }
    let (settings, layout) = state.snapshot().await;
    let web = server_kind(settings.primary_server());
    let result = if !state.supervisor.is_running(web).await {
        let installed = state.supervisor.list(&settings, &layout).await.iter().any(|s| s.kind == web && s.installed);
        if !installed {
            return;
        }
        logs::system(app, logs::Level::Info, format!("Starting {} so {}'s .test address works", web.label(), kind.label()));
        state.supervisor.start(app, web, &settings, &layout).await
    } else {
        let served = sites::served(&settings, &layout);
        let plan = php::plan(&settings, &layout, &served);
        if !sites::config_stale(&settings, &layout, &served, &plan, settings.primary_server()) {
            return;
        }
        // A reload doesn't touch the hosts file the way a start does.
        services::sync_hosts(app, &settings, &layout, &served).await;
        match state.supervisor.reload(app, web, &settings, &layout).await {
            Ok(true) => Ok(()),
            // No graceful reload (Apache on Windows): a quick restart instead.
            Ok(false) => {
                state.supervisor.stop(app, web).await;
                state.supervisor.wait_stopped(web).await;
                state.supervisor.start(app, web, &settings, &layout).await
            }
            Err(e) => Err(e),
        }
    };
    if let Err(e) = result {
        logs::system(app, logs::Level::Warn, format!("{}'s .test address may not work yet: {e}", kind.label()));
    }
}

#[tauri::command]
async fn stop_service(app: AppHandle, state: State<'_, AppState>, kind: ServiceKind) -> Result<()> {
    let _op = state.ops.lock().await;
    state.supervisor.stop(&app, kind).await;
    // Finish stopping before the next queued operation looks at the state.
    state.supervisor.wait_stopped(kind).await;
    Ok(())
}

/// Starts every installed service; returns one error message per failure.
#[tauri::command]
async fn start_all(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<String>> {
    let _op = state.ops.lock().await;
    let (settings, layout) = state.snapshot().await;
    let mut errors = Vec::new();
    for info in state.supervisor.list(&settings, &layout).await {
        if !info.installed || !info.kind.enabled(&settings) {
            continue;
        }
        if let Err(e) = state.supervisor.start(&app, info.kind, &settings, &layout).await {
            errors.push(format!("{}: {e}", info.label));
        }
    }
    Ok(errors)
}

/// What removing LampRabit deletes and keeps, for Settings to show before asking.
#[tauri::command]
async fn uninstall_plan(app: AppHandle, state: State<'_, AppState>) -> Result<uninstall::Plan> {
    let (_, layout) = state.snapshot().await;
    let config_dir = app.path().app_config_dir().map_err(|e| Error::msg(e.to_string()))?;
    Ok(uninstall::plan(&layout, &config_dir))
}

/// Removes LampRabit's files and outside changes, keeping www/ and data/. The app is left
/// without settings or runtimes, so Settings quits it afterwards.
#[tauri::command]
async fn uninstall(app: AppHandle, state: State<'_, AppState>) -> Result<uninstall::Report> {
    let _op = state.ops.lock().await;
    state.tunnels.stop_all().await;
    // A clean stop, so the databases kept in data/ shut down properly.
    for kind in ServiceKind::all() {
        state.supervisor.stop(&app, kind).await;
    }
    for kind in ServiceKind::all() {
        state.supervisor.wait_stopped(kind).await;
    }
    let (_, layout) = state.snapshot().await;
    let config_dir = app.path().app_config_dir().map_err(|e| Error::msg(e.to_string()))?;
    Ok(uninstall::run(&layout, &config_dir).await)
}

#[tauri::command]
async fn stop_all(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let _op = state.ops.lock().await;
    for kind in ServiceKind::all() {
        state.supervisor.stop(&app, kind).await;
    }
    for kind in ServiceKind::all() {
        state.supervisor.wait_stopped(kind).await;
    }
    Ok(())
}

/// Stops the project processes in `stop`, then starts those in `start` ("project/process"),
/// for stack profiles. Returns one message per process that failed to start; a process
/// that no longer exists is skipped.
#[tauri::command]
async fn apply_processes(app: AppHandle, state: State<'_, AppState>, start: Vec<String>, stop: Vec<String>) -> Result<Vec<String>> {
    for key in &stop {
        state.apps.stop(&app, key).await;
    }
    let (settings, layout) = state.snapshot().await;
    let mut errors = Vec::new();
    for key in &start {
        let Some((project, process)) = key.split_once('/') else { continue };
        let exists = settings.sites.get(project).is_some_and(|s| s.processes.iter().any(|p| p.name == process));
        if !exists {
            continue;
        }
        if let Err(e) = state.apps.start_process(&app, &settings, &layout, project, process).await {
            errors.push(format!("{key}: {e}"));
        }
    }
    Ok(errors)
}

/// Stops `stop`, then starts `start` (installed and enabled ones only), in one go: what
/// selection actions, stack profiles and Start/Stop all run. Returns one message per
/// service that failed to start.
#[tauri::command]
async fn apply_services(
    app: AppHandle,
    state: State<'_, AppState>,
    start: Vec<ServiceKind>,
    stop: Vec<ServiceKind>,
) -> Result<Vec<String>> {
    let _op = state.ops.lock().await;
    for &kind in &stop {
        state.supervisor.stop(&app, kind).await;
    }
    for &kind in &stop {
        state.supervisor.wait_stopped(kind).await;
    }
    let (settings, layout) = state.snapshot().await;
    let mut errors = Vec::new();
    for info in state.supervisor.list(&settings, &layout).await {
        if !start.contains(&info.kind) || !info.installed || !info.kind.enabled(&settings) {
            continue;
        }
        if let Err(e) = state.supervisor.start(&app, info.kind, &settings, &layout).await {
            errors.push(format!("{}: {e}", info.label));
        }
    }
    // Started together with the web server (Start all), the domains are already in its
    // config; on their own (a profile without it), it's brought up for them.
    for kind in start.into_iter().filter(|k| tool_domain_service(*k)) {
        if state.supervisor.is_running(kind).await {
            serve_tool_domain(&app, &state, kind).await;
        }
    }
    Ok(errors)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Usage {
    /// A service id ("apache", "mariadb"…) or "php" for the PHP workers.
    id: String,
    /// CPU like `top` shows it: 100 is one full core.
    cpu: f32,
    /// Resident memory in bytes.
    memory: u64,
    processes: usize,
}

/// CPU and memory of each running service, counting its child processes (Apache and
/// Nginx workers), plus PHP's workers as their own entry.
#[tauri::command]
async fn service_usage(state: State<'_, AppState>) -> Result<Vec<Usage>> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, UpdateKind};
    let (settings, layout) = state.snapshot().await;
    let roots: Vec<(ServiceKind, u32)> =
        state.supervisor.list(&settings, &layout).await.into_iter().filter_map(|s| Some((s.kind, s.pid?))).collect();
    let php_bin = layout.bin("php", "");

    let mut system = state.processes.lock().unwrap_or_else(|e| e.into_inner());
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cpu().with_memory().with_exe(UpdateKind::OnlyIfNotSet),
    );
    let mut children: std::collections::HashMap<Pid, Vec<Pid>> = std::collections::HashMap::new();
    // On Linux every thread is listed too, each with its whole process's memory.
    let processes = || system.processes().iter().filter(|(_, p)| p.thread_kind().is_none());
    for (pid, process) in processes() {
        if let Some(parent) = process.parent() {
            children.entry(parent).or_default().push(*pid);
        }
    }
    let mut out = Vec::new();
    for (kind, root) in roots {
        let mut usage = Usage { id: kind.id().to_string(), cpu: 0.0, memory: 0, processes: 0 };
        let mut queue = vec![Pid::from_u32(root)];
        while let Some(pid) = queue.pop() {
            if let Some(p) = system.process(pid).filter(|p| p.thread_kind().is_none()) {
                usage.cpu += p.cpu_usage();
                usage.memory += p.memory();
                usage.processes += 1;
            }
            queue.extend(children.get(&pid).into_iter().flatten().copied());
        }
        out.push(usage);
    }
    let mut php = Usage { id: "php".into(), cpu: 0.0, memory: 0, processes: 0 };
    for (_, p) in processes().filter(|(_, p)| p.exe().is_some_and(|e| e.starts_with(&php_bin))) {
        php.cpu += p.cpu_usage();
        php.memory += p.memory();
        php.processes += 1;
    }
    if php.processes > 0 {
        out.push(php);
    }
    Ok(out)
}

#[tauri::command]
async fn restart_service(app: AppHandle, state: State<'_, AppState>, kind: ServiceKind) -> Result<()> {
    let _op = state.ops.lock().await;
    logs::system(&app, logs::Level::Info, format!("Restarting {}", kind.label()));
    state.supervisor.stop(&app, kind).await;
    state.supervisor.wait_stopped(kind).await;
    let (settings, layout) = state.snapshot().await;
    state.supervisor.start(&app, kind, &settings, &layout).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SitesInfo {
    sites: Vec<sites::Site>,
    /// Installed developer tools and their domains.
    tools: Vec<sites::Site>,
    tld: String,
    /// Apache is running with a different site list than what is in www/ now.
    needs_apply: bool,
    /// The hosts file is in sync, or does not need to be (`.localhost`, or management off).
    hosts_ok: bool,
    manage_hosts: bool,
    /// None while no site uses HTTPS.
    ca_trusted: Option<bool>,
    ca_path: PathBuf,
    /// Folders in www/ that can't be served, with the reason.
    unserved: Vec<sites::Unserved>,
    /// Tool domains (adminer.test…) that a folder of the same name has taken.
    shadowed_tools: Vec<String>,
}

/// Problems found while starting up, shown once the window is ready.
struct StartupWarnings(std::sync::Mutex<Vec<String>>);

#[tauri::command]
fn take_startup_warnings(warnings: State<'_, StartupWarnings>) -> Vec<String> {
    std::mem::take(&mut *warnings.0.lock().unwrap())
}

/// Every folder in www/ is a site, most recently modified first.
#[tauri::command]
async fn list_sites(state: State<'_, AppState>) -> Result<SitesInfo> {
    let (settings, layout) = state.snapshot().await;
    let served = sites::served(&settings, &layout);
    let plan = php::plan(&settings, &layout, &served);
    let mut needs_apply = false;
    for kind in ServiceKind::WEB {
        if state.supervisor.is_running(kind).await {
            needs_apply |= sites::config_stale(&settings, &layout, &served, &plan, kind.web_server().unwrap());
        }
    }
    let hosts_ok = !sites::needs_hosts_file(&settings) || sites::hosts_in_sync(&served);
    let (tools, sites): (Vec<_>, Vec<_>) = served.into_iter().partition(|s| s.tool.is_some());
    // A project's subdomains are served as copies of it (for configs, certificates and the
    // hosts file); the app shows them on their project, not as projects of their own.
    let sites: Vec<_> = sites.into_iter().filter(|s| !sites::is_extra(s)).collect();
    let ca_trusted = if sites.iter().any(|s| s.ssl) && ssl::ca_thumbprint(&layout).is_some() {
        let mut cached = state.ca_trusted.lock().await;
        if cached.is_none() {
            *cached = Some(ssl::ca_trusted(&layout).await);
        }
        *cached
    } else if sites.iter().any(|s| s.ssl) {
        Some(false)
    } else {
        None
    };
    Ok(SitesInfo {
        sites,
        tools,
        tld: settings.tld.clone(),
        needs_apply,
        hosts_ok,
        manage_hosts: settings.manage_hosts,
        ca_trusted,
        ca_path: ssl::ca_cert_path(&layout),
        unserved: sites::unserved(&settings, &layout),
        shadowed_tools: sites::shadowed_tools(&settings, &layout),
    })
}

/// Makes Apache and the hosts file match www/: restarts Apache if it is running
/// (which regenerates vhosts and certificates), otherwise just syncs the hosts file.
#[tauri::command]
async fn apply_sites(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let _op = state.ops.lock().await;
    logs::system(&app, logs::Level::Info, "Applying site changes");
    sync_sites(&app, &state).await
}

/// Restarts the running web servers (vhosts, certificates) or, with none running, syncs
/// the hosts file; for changes to the list of projects.
async fn sync_sites(app: &AppHandle, state: &AppState) -> Result<()> {
    let (settings, layout) = state.snapshot().await;
    let app = app.clone();
    let mut any_running = false;
    for kind in ServiceKind::WEB {
        any_running |= state.supervisor.is_running(kind).await;
    }
    if any_running {
        restart_web_servers(&app, &state).await
    } else {
        let sites = sites::served(&settings, &layout);
        if sites::needs_hosts_file(&settings) && !sites::hosts_in_sync(&sites) {
            sites::update_hosts(&layout, &sites).await?;
            logs::system(&app, logs::Level::Info, "Hosts file updated");
        }
        Ok(())
    }
}

#[tauri::command]
async fn set_site_ssl(app: AppHandle, state: State<'_, AppState>, name: String, ssl: bool) -> Result<()> {
    let _op = state.ops.lock().await;
    state.update_settings(|s| s.sites.entry(name.clone()).or_default().ssl = Some(ssl)).await?;
    logs::system(&app, logs::Level::Info, format!("HTTPS {} for {name}", if ssl { "enabled" } else { "disabled" }));
    restart_web_servers(&app, &state).await
}

/// Sets a site's PHP version (None = follow the active version).
#[tauri::command]
async fn set_site_php(app: AppHandle, state: State<'_, AppState>, name: String, version: Option<String>) -> Result<()> {
    let _op = state.ops.lock().await;
    let (_, layout) = state.snapshot().await;
    if let Some(v) = &version {
        if !runtimes::installed(&layout, Runtime::Php).contains(v) {
            return Err(Error::msg(format!("PHP {v} is not installed")));
        }
    }
    let label = version.clone().unwrap_or_else(|| "the active version".into());
    state.update_settings(|s| s.sites.entry(name.clone()).or_default().php = version).await?;
    logs::system(&app, logs::Level::Info, format!("{name} now uses PHP {label}"));
    restart_web_servers(&app, &state).await
}

/// Turns FrankenPHP's worker mode on or off for a site.
#[tauri::command]
async fn set_site_worker(app: AppHandle, state: State<'_, AppState>, name: String, enabled: bool) -> Result<()> {
    let _op = state.ops.lock().await;
    state.update_settings(|s| s.sites.entry(name.clone()).or_default().worker = enabled).await?;
    logs::system(&app, logs::Level::Info, format!("Worker mode {} for {name}", if enabled { "on" } else { "off" }));
    restart_web_servers(&app, &state).await
}

/// Reloads the code of FrankenPHP's workers (they keep the app in memory between requests)
/// through its admin API, without stopping the server.
#[tauri::command]
async fn restart_frankenphp_workers(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    if !state.supervisor.is_running(ServiceKind::Frankenphp).await {
        return Err(Error::msg("FrankenPHP isn't running; its workers load the current code when it starts."));
    }
    let (settings, _) = state.snapshot().await;
    let url = format!("http://127.0.0.1:{}/frankenphp/workers/restart", settings.ports.frankenphp_admin);
    let response = state.http.post(&url).send().await.map_err(|e| Error::msg(format!("Couldn't reach FrankenPHP's admin API: {e}")))?;
    if !response.status().is_success() {
        return Err(Error::msg(format!("FrankenPHP didn't restart its workers ({}). Restart FrankenPHP on Home instead.", response.status())));
    }
    logs::system(&app, logs::Level::Info, "FrankenPHP's workers restarted");
    Ok(())
}

/// Picks the web server for a site when both are enabled.
#[tauri::command]
async fn set_site_server(app: AppHandle, state: State<'_, AppState>, name: String, server: Option<config::Server>) -> Result<()> {
    let _op = state.ops.lock().await;
    state.update_settings(|s| s.sites.entry(name.clone()).or_default().server = server).await?;
    logs::system(&app, logs::Level::Info, format!("{name} is now served by {}", server.map_or("the default server", |s| s.label())));
    restart_web_servers(&app, &state).await?;
    // The site now needs its new server; start it when a web server was serving sites.
    let (settings, layout) = state.snapshot().await;
    let kind = server_kind(sites::site_server(&settings, &name));
    let mut others_running = false;
    for other in ServiceKind::WEB.into_iter().filter(|&k| k != kind) {
        others_running |= state.supervisor.is_running(other).await;
    }
    if !state.supervisor.is_running(kind).await && others_running {
        state.supervisor.start(&app, kind, &settings, &layout).await?;
    }
    Ok(())
}

/// Creates a project in www/ from a template and returns its URL. Progress is streamed as
/// `project-progress` events; a failed project is removed so it can be retried.
#[tauri::command]
async fn create_project(app: AppHandle, state: State<'_, AppState>, request: projects::NewProject) -> Result<CreatedProject> {
    let (_, layout) = state.snapshot().await;
    projects::validate(&request, &layout)?;
    let (template, source) = templates::find(&layout, &request.template)?;
    let name = request.name.trim().to_string();
    state
        .update_settings(|s| {
            let site = s.sites.entry(name.clone()).or_default();
            site.php = request.php.clone();
            site.server = request.server;
            site.ssl = Some(request.ssl);
            site.template = Some(format!("{}@{}", template.id, template.version));
        })
        .await?;
    let (settings, layout) = state.snapshot().await;
    let url = sites::url_for(&settings, &name).ok_or_else(|| Error::msg("invalid project name"))?;
    let ctx = projects::Ctx { app: &app, http: &state.http, settings: &settings, layout: &layout, name: &name };
    logs::system(&app, logs::Level::Info, format!("Creating project {name} from the {} template", template.name));

    let result = async {
        let creates_database = template.steps.iter().any(|s| matches!(s.action, templates::Action::CreateDatabase(_)));
        if request.database.is_some() && creates_database {
            use templates::DbEngine;
            let engine = projects::engine(&settings, &request).ok_or_else(|| Error::msg("Install MariaDB, MySQL or PostgreSQL first (Services)."))?;
            if !template.engines.contains(&engine) {
                let supported: Vec<&str> = template.engines.iter().map(|e| e.label()).collect();
                return Err(Error::msg(format!("{} projects can use {}, not {}.", template.name, supported.join(" or "), engine.label())));
            }
            let kind = match engine {
                DbEngine::Mariadb => ServiceKind::Mariadb,
                DbEngine::Mysql => ServiceKind::Mysql,
                DbEngine::Postgres => ServiceKind::Postgres,
                DbEngine::Mongodb => ServiceKind::Mongodb,
                // Nothing to start: PHP has SQLite built in.
                DbEngine::Sqlite => return projects::build(&ctx, &request, &template, source, &url).await,
            };
            if settings.versions.get(engine.id()).is_none() {
                return Err(Error::msg(format!("Install {} first (Services).", engine.label())));
            }
            if !state.supervisor.is_running(kind).await {
                ctx.step(format!("Starting {}", engine.label()));
                state.supervisor.start(&app, kind, &settings, &layout).await?;
            }
            // The process is up before it accepts connections.
            for _ in 0..30 {
                let probe = match engine {
                    DbEngine::Postgres => diagnose::probe_postgres(settings.ports.postgres).await,
                    DbEngine::Mongodb | DbEngine::Sqlite => diagnose::Probe::Ok,
                    DbEngine::Mysql => diagnose::probe_mysql(settings.ports.mysql_server).await,
                    DbEngine::Mariadb => diagnose::probe_mysql(settings.ports.mysql).await,
                };
                if matches!(probe, diagnose::Probe::Ok) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
        projects::build(&ctx, &request, &template, source, &url).await
    }
    .await;

    let built = match result {
        Ok(built) => built,
        Err(e) => {
            let _ = std::fs::remove_dir_all(layout.www().join(&name));
            state.update_settings(|s| { s.sites.remove(&name); }).await?;
            logs::system(&app, logs::Level::Error, format!("Creating project {name} failed: {e}"));
            return Err(e);
        }
    };
    // A Node project gets its run setup, on a port of its own.
    if let Some(mut app_config) = built.app {
        app_config.port = apps::free_port(&*state.settings.read().await);
        state.update_settings(|s| s.sites.entry(name.clone()).or_default().app = Some(app_config)).await?;
    }
    // The template's processes ("auto" ports picked one by one, so they differ) and subdomains.
    if !built.processes.is_empty() || !built.domains.is_empty() {
        let mut scratch = state.settings.read().await.clone();
        let mut processes = built.processes;
        for i in 0..processes.len() {
            if processes[i].port == Some(0) {
                processes[i].port = Some(apps::free_port(&scratch));
            }
            scratch.sites.entry(name.clone()).or_default().processes = processes[..=i].to_vec();
        }
        let parent = format!("{}.{}", sites::label(&name).unwrap_or_default(), scratch.tld);
        let domains: Vec<config::ExtraDomain> = built.domains.into_iter().map(|(sub, target)| config::ExtraDomain { host: format!("{sub}.{parent}"), target }).collect();
        state
            .update_settings(|s| {
                let own = s.sites.entry(name.clone()).or_default();
                own.processes = processes;
                own.domains = domains;
            })
            .await?;
    }
    // Composer may have just been installed; this makes it the active version and adds its shim.
    state.update_settings(reconcile_versions).await?;
    ctx.step("Setting up HTTPS and the hosts file");
    prepare_new_site(&app, &state, &name).await;
    ctx.step("Adding the site to the web server");
    restart_web_servers(&app, &state).await?;
    logs::system(&app, logs::Level::Info, format!("Created project {name} at {url}"));
    Ok(CreatedProject { url, note: built.note })
}

#[derive(Serialize)]
struct CreatedProject {
    url: String,
    /// The template's "what next", e.g. finishing the WordPress installer.
    note: Option<String>,
}

/// Every project template: built-in, then the user's, with those that couldn't be read.
#[tauri::command]
async fn list_templates(state: State<'_, AppState>) -> Result<templates::Catalog> {
    let (_, layout) = state.snapshot().await;
    Ok(templates::catalog(&layout))
}

/// A template that recreates the project, as YAML for the template editor.
#[tauri::command]
async fn template_from_project(state: State<'_, AppState>, name: String) -> Result<String> {
    let (settings, layout) = state.snapshot().await;
    let dir = project_dir(&layout, &name)?;
    let site = sites::scan(&settings, &layout).into_iter().find(|s| s.name == name);
    let facts = facts::read(&dir, site.as_ref().and_then(|s| s.framework.as_deref()), "npm").await;
    let php = site.as_ref().filter(|s| s.language == sites::Language::Php).and_then(|s| s.php.clone().or_else(|| settings.versions.php.clone()));
    let has_database = site.as_ref().is_some_and(|s| s.database.is_some());
    Ok(templategen::generate(&templategen::Setup {
        name: &name,
        dir: &dir,
        remote: facts.git.and_then(|g| g.remote),
        php,
        engine: has_database.then(|| manage::engine(&settings, &dir)).flatten(),
        has_database,
        own: settings.sites.get(&name),
        parent: format!("{}.{}", sites::label(&name).unwrap_or_default(), settings.tld),
    }))
}

/// Saves a template's YAML into the templates folder after the same checks loading does.
/// An existing template with its id is replaced only when `replace` says so.
#[tauri::command]
async fn save_template(app: AppHandle, state: State<'_, AppState>, yaml: String, replace: bool) -> Result<String> {
    let (_, layout) = state.snapshot().await;
    let template = templates::parse_text(&yaml).map_err(|e| Error::msg(format!("The template isn't valid yet: {e}")))?;
    let dir = templates::folder(&layout);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.yaml", template.id));
    if path.exists() && !replace {
        return Err(Error::msg(format!("You already have a template with the id {}. Change the id, or replace it.", template.id)));
    }
    std::fs::write(&path, yaml)?;
    logs::system(&app, logs::Level::Info, format!("Saved template {} ({})", template.name, path.display()));
    Ok(template.id)
}

/// Opens the folder for your own templates, creating it with an example the first time.
#[tauri::command]
async fn open_templates_folder(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    use tauri_plugin_opener::OpenerExt;
    let (_, layout) = state.snapshot().await;
    let dir = templates::folder(&layout);
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("example.yaml.txt"), templates::EXAMPLE)?;
    }
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| Error::msg(e.to_string()))
}

/// A project folder in www/ by name; names with path separators are refused.
fn project_dir(layout: &Layout, name: &str) -> Result<PathBuf> {
    let dir = layout.www().join(name);
    if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') || !dir.is_dir() {
        return Err(Error::msg(format!("There's no project called {name} in www.")));
    }
    Ok(dir)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    /// The saved setup, or what package.json suggests when there's none yet (port 0: picked
    /// when it starts).
    config: config::NodeApp,
    configured: bool,
    scripts: std::collections::BTreeMap<String, String>,
    detected_package_manager: config::PackageManager,
    /// Versions installed with NVM, newest first.
    node_versions: Vec<String>,
    /// What .nvmrc / .node-version asks for.
    project_node: Option<String>,
    framework: Option<&'static str>,
    status: apps::AppStatus,
}

/// How a Node project runs, for its panel on the Sites page.
#[tauri::command]
async fn app_info(state: State<'_, AppState>, name: String) -> Result<AppInfo> {
    let (settings, layout) = state.snapshot().await;
    let dir = project_dir(&layout, &name)?;
    let saved = settings.sites.get(&name).and_then(|s| s.app.clone());
    Ok(AppInfo {
        configured: saved.is_some(),
        config: saved.unwrap_or_else(|| apps::suggest(&dir, 0)),
        scripts: apps::scripts(&dir),
        detected_package_manager: apps::detect_package_manager(&dir),
        node_versions: node::installed_versions(),
        project_node: node::project_version(&dir),
        framework: apps::framework(&dir),
        status: state.apps.status(&name).await,
    })
}

/// Saves how a Node project runs. Serving it (or its port) changing rewrites the web
/// server's sites, which a running web server picks up with a restart.
#[tauri::command]
async fn app_save(app: AppHandle, state: State<'_, AppState>, name: String, config: config::NodeApp) -> Result<()> {
    let (settings, layout) = state.snapshot().await;
    project_dir(&layout, &name)?;
    // Port 0 means "pick one when it starts".
    if let Some((other, _)) = settings.sites.iter().find(|(n, s)| config.port > 0 && **n != name && s.app.as_ref().is_some_and(|a| a.port == config.port)) {
        return Err(Error::msg(format!("Port {} is already {other}'s. Choose another one.", config.port)));
    }
    let before = settings.sites.get(&name).and_then(|s| s.app.clone());
    let routing = |a: &Option<config::NodeApp>| a.as_ref().map(|a| (a.serve, a.port));
    let changed = routing(&before) != routing(&Some(config.clone()));
    state.update_settings(|s| s.sites.entry(name.clone()).or_default().app = Some(config)).await?;
    if changed {
        restart_web_servers(&app, &state).await?;
    }
    Ok(())
}

/// Gets a project ready to start: a free port (the saved one while it's still free) and
/// the web server for its domain running. The domain switches to the app once its port
/// listens (see `watch_ports`). Returns a note when the web server couldn't start; the app
/// still runs on its port then.
async fn prepare_app(app: &AppHandle, state: &AppState, name: &str) -> Result<Option<String>> {
    let (settings, layout) = state.snapshot().await;
    let dir = project_dir(&layout, name)?;
    let saved = settings.sites.get(name).and_then(|s| s.app.clone());
    let mut config = saved.clone().unwrap_or_else(|| apps::suggest(&dir, 0));
    let taken = settings.sites.iter().any(|(n, s)| n != name && s.app.as_ref().is_some_and(|a| a.port == config.port));
    if config.port == 0 || taken || std::net::TcpListener::bind(("127.0.0.1", config.port)).is_err() {
        config.port = apps::free_port(&settings);
    }
    if saved.as_ref() != Some(&config) {
        let to_save = config.clone();
        state.update_settings(|s| s.sites.entry(name.to_string()).or_default().app = Some(to_save)).await?;
    }
    if !config.serve || matches!(sites::language(&dir), sites::Language::Php | sites::Language::Static) {
        return Ok(None);
    }
    let kind = server_kind(sites::site_server(&settings, name));
    let _op = state.ops.lock().await;
    if state.supervisor.is_running(kind).await {
        return Ok(None);
    }
    // The domain needs the web server; it starts with the site (and its certificate).
    let (settings, layout) = state.snapshot().await;
    Ok(state
        .supervisor
        .start(app, kind, &settings, &layout)
        .await
        .err()
        .map(|e| format!("{} didn't start, so {} isn't reachable at its domain yet: {e}", kind.label(), name)))
}

fn server_kind(server: config::Server) -> ServiceKind {
    ServiceKind::for_server(server)
}

/// Keeps every project's domain pointed at what it's serving: the port its processes
/// listen on (a dev server from LampRabit or your own terminal), or its files when nothing
/// listens. Runs every few seconds, and straight away when an app starts or stops.
/// Processes set to start with LampRabit, then, once a minute, the ones whose schedule
/// fires (a scheduled process already running is left alone).
fn spawn_process_schedules(handle: &AppHandle) {
    let handle = handle.clone();
    tauri::async_runtime::spawn(async move {
        let state = handle.state::<AppState>();
        let (settings, layout) = state.snapshot().await;
        for (project, own) in &settings.sites {
            for p in own.processes.iter().filter(|p| p.at_launch) {
                if let Err(e) = state.apps.start_process(&handle, &settings, &layout, project, &p.name).await {
                    logs::system(&handle, logs::Level::Warn, &format!("{project}/{}: couldn't start on launch: {e}", p.name));
                }
            }
        }
        loop {
            // Wake just after each minute turns, so a "0 3 * * *" job runs at 3:00, not 3:00:59.
            let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
            tokio::time::sleep(std::time::Duration::from_secs(61 - u64::from(now.second()))).await;
            let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
            let (settings, layout) = state.snapshot().await;
            for (project, own) in &settings.sites {
                for p in &own.processes {
                    let Ok(schedule) = cron::Schedule::parse(p.schedule.trim()) else { continue };
                    if schedule.matches(now) {
                        if let Err(e) = state.apps.start_process(&handle, &settings, &layout, project, &p.name).await {
                            logs::system(&handle, logs::Level::Warn, &format!("{project}/{}: the scheduled start failed: {e}", p.name));
                        }
                    }
                }
            }
        }
    });
}

async fn watch_ports(handle: AppHandle) {
    let mut system = sysinfo::System::new();
    // A route is dropped only after its port has been gone for two scans, so a dev server
    // restarting itself (on a config change) doesn't flip the site to its files and back.
    let mut missing: std::collections::BTreeMap<String, u8> = Default::default();
    // What the projects' version files ask for; the shims are rewritten when it changes.
    let mut versions = String::new();
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(3)) => {}
            _ = ports::WAKE.notified() => {}
        }
        let state = handle.state::<AppState>();
        let (settings, layout) = state.snapshot().await;
        let now = langver::signature(&layout);
        if now != versions {
            if !versions.is_empty() {
                refresh_shims(&handle, &settings);
            }
            versions = now;
        }
        let sites = sites::scan(&settings, &layout);
        let projects: Vec<(String, PathBuf)> = sites.iter().map(|s| (s.name.clone(), s.path.clone())).collect();
        let Ok((found, back)) = tauri::async_runtime::spawn_blocking(move || {
            let found = ports::discover(&mut system, &projects);
            (found, system)
        })
        .await
        else {
            system = sysinfo::System::new();
            continue;
        };
        system = back;

        let statuses = state.apps.statuses().await;
        let current = ports::routes();
        let mut next = current.clone();
        let mut servers: Vec<ServiceKind> = Vec::new();
        let mut notes: Vec<String> = Vec::new();
        // Servers a newly routed domain needs; started when they aren't running, since the
        // dev server is only reachable at its domain through them.
        let mut needed: Vec<ServiceKind> = Vec::new();
        for site in &sites {
            let own = settings.sites.get(&site.name);
            let files_only = site.language != sites::Language::Php && own.and_then(|s| s.app.as_ref()).is_some_and(|a| !a.serve);
            let status = statuses.iter().find(|s| s.name == site.name);
            let managed = status.filter(|s| matches!(s.state, "starting" | "running")).and_then(|s| s.port);
            // The project's processes listen on their own ports (Vite, an API); those go to
            // the domains pointing at them, not to the main domain, unless it's pinned there.
            let pinned = own.and_then(|s| s.route_port);
            let reserved: Vec<u16> = own.map(|s| s.processes.iter().filter_map(|p| p.port).filter(|p| Some(*p) != pinned).collect()).unwrap_or_default();
            let detected: Vec<ports::Detected> = found.get(&site.name).map(|d| d.iter().filter(|d| !reserved.contains(&d.port)).cloned().collect()).unwrap_or_default();
            let detected = detected.as_slice();
            if let (Some(s), Some(first)) = (status, detected.first()) {
                if s.state == "starting" {
                    let port = managed.filter(|p| detected.iter().any(|d| d.port == *p)).unwrap_or(first.port);
                    state.apps.mark_ready(&handle, &site.name, port).await;
                }
            }
            // A main domain set by hand goes where it was told (sites::scan), not where ports lead.
            if own.is_some_and(|s| s.main.is_some()) {
                next.remove(&site.name);
                missing.remove(&site.name);
                continue;
            }
            let want = ports::choose(site.language, files_only, own.and_then(|s| s.route_port), managed, detected);
            if want.as_ref() == current.get(&site.name) {
                missing.remove(&site.name);
                continue;
            }
            if want.is_none() {
                let gone = missing.entry(site.name.clone()).or_default();
                *gone += 1;
                if *gone < 2 {
                    continue;
                }
            }
            missing.remove(&site.name);
            let note = match &want {
                Some(r) => format!("{} is now served from port {}", site.domain, r.port),
                None => format!("{}'s dev server stopped; serving its files", site.domain),
            };
            logs::system(&handle, logs::Level::Info, note.clone());
            notes.push(note);
            let kind = server_kind(site.server);
            if want.is_some() && !needed.contains(&kind) {
                needed.push(kind);
            }
            match want {
                Some(r) => next.insert(site.name.clone(), r),
                None => next.remove(&site.name),
            };
            if !servers.contains(&kind) {
                servers.push(kind);
            }
        }
        // Projects that disappeared lose their routes quietly.
        next.retain(|name, _| sites.iter().any(|s| &s.name == name));
        let detected_changed = ports::set_detected(found);
        if next != current {
            ports::set_routes(next);
            let _op = state.ops.lock().await;
            for kind in servers {
                if state.supervisor.is_running(kind).await {
                    // Without dropping connections where the server can; a restart otherwise.
                    let (settings, layout) = state.snapshot().await;
                    match state.supervisor.reload(&handle, kind, &settings, &layout).await {
                        Ok(true) => continue,
                        Ok(false) => {}
                        Err(e) => {
                            logs::system(&handle, logs::Level::Error, e.to_string());
                            continue;
                        }
                    }
                    state.supervisor.stop(&handle, kind).await;
                    state.supervisor.wait_stopped(kind).await;
                    let (settings, layout) = state.snapshot().await;
                    if let Err(e) = state.supervisor.start(&handle, kind, &settings, &layout).await {
                        logs::system(&handle, logs::Level::Error, format!("{} didn't restart with the new routes: {e}", kind.label()));
                    }
                } else if needed.contains(&kind) {
                    let (settings, layout) = state.snapshot().await;
                    if let Err(e) = state.supervisor.start(&handle, kind, &settings, &layout).await {
                        let note = format!("{} didn't start, so the domain can't reach the dev server yet: {e}", kind.label());
                        logs::system(&handle, logs::Level::Error, note.clone());
                        notes.push(note);
                    }
                }
            }
            // The notes say where domains moved (shown as notifications).
            let _ = tauri::Emitter::emit(&handle, "routes-changed", &notes);
        } else if detected_changed {
            let _ = tauri::Emitter::emit(&handle, "routes-changed", Vec::<String>::new());
        }
    }
}

/// Picks the port a project's domain goes to when it listens on several; None goes back
/// to choosing automatically.
#[tauri::command]
async fn set_site_route(state: State<'_, AppState>, name: String, port: Option<u16>) -> Result<()> {
    let (_, layout) = state.snapshot().await;
    project_dir(&layout, &name)?;
    state.update_settings(|s| s.sites.entry(name.clone()).or_default().route_port = port).await?;
    ports::poke();
    Ok(())
}

/// Starts a project's dev server and serves its domain through it.
#[tauri::command]
async fn app_start(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<apps::AppStatus> {
    let status = state.apps.status(&name).await;
    if matches!(status.state, "starting" | "running") {
        return Ok(status);
    }
    let note = prepare_app(&app, &state, &name).await?;
    let (settings, layout) = state.snapshot().await;
    let status = state.apps.start(&app, &settings, &layout, &name).await?;
    if let Some(note) = note {
        state.apps.record(&app, &layout, &name, "error", note).await;
    }
    // Its processes that run with it; one failing doesn't stop the app (its log says why).
    let _ = state.apps.start_with_app(&app, &settings, &layout, &name).await;
    Ok(status)
}

#[tauri::command]
async fn app_stop(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<()> {
    state.apps.stop(&app, &name).await;
    let (settings, _) = state.snapshot().await;
    state.apps.stop_processes(&app, &settings, &name, true).await;
    Ok(())
}

/// Saves a project's extra domains and serves them (new certificates and hosts entries
/// come with the web server's restart).
#[tauri::command]
async fn set_site_domains(app: AppHandle, state: State<'_, AppState>, name: String, domains: Vec<config::ExtraDomain>) -> Result<()> {
    let _op = state.ops.lock().await;
    let (settings, layout) = state.snapshot().await;
    project_dir(&layout, &name)?;
    let domains: Vec<config::ExtraDomain> = domains
        .into_iter()
        .map(|d| config::ExtraDomain { host: d.host.trim().trim_end_matches('.').to_lowercase(), target: d.target })
        .collect();
    sites::check_domains(&settings, &layout, &name, &domains)?;
    let count = domains.len();
    state.update_settings(|s| s.sites.entry(name.clone()).or_default().domains = domains).await?;
    logs::system(&app, logs::Level::Info, format!("{name} has {count} extra domain{}", if count == 1 { "" } else { "s" }));
    restart_web_servers(&app, &state).await
}

/// Sets where a project's main domain goes: by hand (its files or a folder, a port, a
/// process), or None to follow what's running.
#[tauri::command]
async fn set_site_main(app: AppHandle, state: State<'_, AppState>, name: String, target: Option<config::DomainTarget>) -> Result<()> {
    let _op = state.ops.lock().await;
    let (settings, layout) = state.snapshot().await;
    project_dir(&layout, &name)?;
    let target = target.filter(|t| *t != config::DomainTarget::Main);
    if let Some(t) = &target {
        let host = sites::url_for(&settings, &name).unwrap_or_else(|| name.clone());
        sites::check_target(&settings, &layout, &name, &host, t)?;
    }
    state.update_settings(|s| s.sites.entry(name.clone()).or_default().main = target).await?;
    ports::poke();
    restart_web_servers(&app, &state).await
}

/// A project's folders, for the domain and process folder pickers.
#[tauri::command]
async fn project_folders(state: State<'_, AppState>, name: String) -> Result<Vec<String>> {
    let (_, layout) = state.snapshot().await;
    let dir = project_dir(&layout, &name)?;
    Ok(sites::project_folders(&dir))
}

/// Saves a project's processes. Removed ones are stopped; a changed one keeps running
/// with its old command until it's restarted.
#[tauri::command]
async fn set_site_processes(app: AppHandle, state: State<'_, AppState>, name: String, processes: Vec<config::ProjectProcess>) -> Result<()> {
    let _op = state.ops.lock().await;
    let (settings, layout) = state.snapshot().await;
    project_dir(&layout, &name)?;
    let mut names = HashSet::new();
    let app_port = settings.sites.get(&name).and_then(|s| s.app.as_ref()).map(|a| a.port);
    let mut ports = HashSet::new();
    for p in &processes {
        if p.name.is_empty() || !p.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') || p.name.starts_with('-') {
            return Err(Error::msg(format!("\"{}\" can't be a process name: use lowercase letters, digits and dashes, like queue or vite.", p.name)));
        }
        if !names.insert(p.name.clone()) {
            return Err(Error::msg(format!("Two processes are called {}.", p.name)));
        }
        if p.command.trim().is_empty() {
            return Err(Error::msg(format!("Enter the command {} runs.", p.name)));
        }
        let plain = std::path::Path::new(&p.cwd).components().all(|c| matches!(c, std::path::Component::Normal(_)));
        if !plain || !layout.www().join(&name).join(&p.cwd).is_dir() {
            return Err(Error::msg(format!("{}: \"{}\" isn't a folder in {name}.", p.name, p.cwd)));
        }
        if !p.schedule.trim().is_empty() {
            cron::Schedule::parse(p.schedule.trim()).map_err(|e| Error::msg(format!("{}: {e}", p.name)))?;
        }
        if let Some(port) = p.port {
            if port < 1024 || Some(port) == app_port || !ports.insert(port) {
                return Err(Error::msg(format!("{}: port {port} is taken (below 1024, the app's, or another process's). Pick another one.", p.name)));
            }
        }
    }
    // A domain can't point at a process that's going away, or that no longer has a port.
    if let Some(own) = settings.sites.get(&name) {
        let main = own.main.iter().map(|t| (name.clone(), t));
        for (host, target) in own.domains.iter().map(|d| (d.host.clone(), &d.target)).chain(main) {
            if let config::DomainTarget::Process { name: target } = target {
                if !processes.iter().any(|p| &p.name == target && p.port.is_some()) {
                    return Err(Error::msg(format!("{host} points to the process {target}. Point it somewhere else first.")));
                }
            }
        }
    }
    let removed: Vec<String> = settings.sites.get(&name).map(|s| s.processes.iter().filter(|old| !names.contains(&old.name)).map(|old| old.name.clone()).collect()).unwrap_or_default();
    for process in &removed {
        state.apps.stop(&app, &apps::process_key(&name, process)).await;
    }
    let ports_changed = settings.sites.get(&name).is_some_and(|s| s.processes.iter().map(|p| (&p.name, p.port)).ne(processes.iter().map(|p| (&p.name, p.port))));
    state.update_settings(|s| s.sites.entry(name.clone()).or_default().processes = processes).await?;
    // Domains that point at a process follow its port.
    let to_process = |t: &config::DomainTarget| matches!(t, config::DomainTarget::Process { .. });
    if ports_changed && settings.sites.get(&name).is_some_and(|s| s.domains.iter().any(|d| to_process(&d.target)) || s.main.as_ref().is_some_and(to_process)) {
        restart_web_servers(&app, &state).await?;
    }
    ports::poke();
    Ok(())
}

#[tauri::command]
async fn process_start(app: AppHandle, state: State<'_, AppState>, name: String, process: String) -> Result<apps::AppStatus> {
    let (settings, layout) = state.snapshot().await;
    state.apps.start_process(&app, &settings, &layout, &name, &process).await
}

#[tauri::command]
async fn process_stop(app: AppHandle, state: State<'_, AppState>, name: String, process: String) -> Result<()> {
    state.apps.stop(&app, &apps::process_key(&name, &process)).await;
    Ok(())
}

#[tauri::command]
async fn process_restart(app: AppHandle, state: State<'_, AppState>, name: String, process: String) -> Result<apps::AppStatus> {
    state.apps.stop(&app, &apps::process_key(&name, &process)).await;
    let (settings, layout) = state.snapshot().await;
    state.apps.start_process(&app, &settings, &layout, &name, &process).await
}

/// Starts (or stops) all of a project's processes, whether or not they run with the app.
#[tauri::command]
async fn processes_all(app: AppHandle, state: State<'_, AppState>, name: String, start: bool) -> Result<()> {
    let (settings, layout) = state.snapshot().await;
    if !start {
        state.apps.stop_processes(&app, &settings, &name, false).await;
        return Ok(());
    }
    let all: Vec<String> = settings.sites.get(&name).map(|s| s.processes.iter().map(|p| p.name.clone()).collect()).unwrap_or_default();
    let mut errors = Vec::new();
    for process in all {
        if let Err(e) = state.apps.start_process(&app, &settings, &layout, &name, &process).await {
            errors.push(format!("{process}: {e}"));
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(Error::msg(errors.join("\n"))) }
}

#[tauri::command]
async fn app_restart(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<apps::AppStatus> {
    state.apps.stop(&app, &name).await;
    app_start(app, state, name).await
}

/// Installs a project's dependencies with its package manager.
#[tauri::command]
async fn app_install(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<()> {
    let (settings, layout) = state.snapshot().await;
    project_dir(&layout, &name)?;
    state.apps.install(&app, &settings, &layout, &name).await
}

#[tauri::command]
async fn app_statuses(state: State<'_, AppState>) -> Result<Vec<apps::AppStatus>> {
    Ok(state.apps.statuses().await)
}

#[tauri::command]
async fn app_logs(state: State<'_, AppState>, name: String) -> Result<Vec<String>> {
    let (_, layout) = state.snapshot().await;
    Ok(state.apps.logs(&layout, &name).await)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectCommands {
    commands: Vec<commands::Command>,
    /// Sources that couldn't be listed, with why.
    problems: Vec<String>,
    sources: Vec<commands::Source>,
}

/// Every command a project can run: yours, its template's, and those discovered from
/// package.json, composer.json and the framework CLIs.
#[tauri::command]
async fn project_commands(state: State<'_, AppState>, name: String) -> Result<ProjectCommands> {
    let (settings, layout) = state.snapshot().await;
    let dir = project_dir(&layout, &name)?;
    let site = settings.sites.get(&name).cloned().unwrap_or_default();
    let template = site.template.as_deref().and_then(|t| templates::find(&layout, t.split('@').next().unwrap_or(t)).ok()).map(|(t, _)| t);
    let declared = template.as_ref().and_then(|t| t.commands.clone()).unwrap_or_default();
    let sources = declared.discover.clone().unwrap_or_else(|| commands::detect(&dir));
    let pm = site.app.as_ref().and_then(|a| a.package_manager).unwrap_or_else(|| apps::detect_package_manager(&dir));
    let path = apps::Apps::command_path(&settings, &layout, &name);
    let (discovered, problems) = commands::discover(&dir, &sources, &path, pm).await;

    // Template commands may use the project's names and package manager.
    let url = sites::url_for(&settings, &name).unwrap_or_default().trim_end_matches('/').to_string();
    let mut values = std::collections::BTreeMap::from([
        ("project".to_string(), name.clone()),
        ("domain".into(), url.split("://").nth(1).unwrap_or_default().to_string()),
        ("url".into(), url),
    ]);
    if let Ok(pmc) = apps::pm_commands(pm, &path) {
        values.extend([
            ("pm".to_string(), pmc.id.to_string()),
            ("pm.run".into(), pmc.run.clone()),
            ("pm.args".into(), pmc.args.into()),
            ("pm.exec".into(), pmc.exec.clone()),
            ("pm.install".into(), pmc.install.clone()),
            ("pm.add".into(), pmc.add.clone()),
        ]);
    }
    let values = templates::Values(values);
    let mut list: Vec<commands::Command> = site
        .commands
        .iter()
        .map(|c| commands::Command { group: "Yours".into(), name: c.name.clone(), command: c.run.clone(), description: None, origin: "yours" })
        .collect();
    if let Some(t) = &template {
        list.extend(declared.list.iter().map(|c| commands::Command {
            group: t.name.clone(),
            name: c.name.clone(),
            command: values.render(&c.run),
            description: c.description.clone(),
            origin: "template",
        }));
    }
    list.extend(discovered);
    Ok(ProjectCommands { commands: list, problems, sources })
}

/// Runs a command in a project's folder; its output goes to the project's log.
#[tauri::command]
async fn run_project_command(app: AppHandle, state: State<'_, AppState>, name: String, label: String, command: String) -> Result<()> {
    let (settings, layout) = state.snapshot().await;
    project_dir(&layout, &name)?;
    if command.trim().is_empty() {
        return Err(Error::msg("Enter a command to run."));
    }
    state.apps.run_command(&app, &settings, &layout, &name, &label, &command).await
}

/// The project's recent command runs, newest first.
#[tauri::command]
async fn command_history(state: State<'_, AppState>, name: String) -> Result<Vec<apps::CommandRun>> {
    let (_, layout) = state.snapshot().await;
    Ok(apps::history(&layout, &name))
}

/// When each of the project's scheduled processes runs next (Unix ms).
#[tauri::command]
async fn process_next_runs(state: State<'_, AppState>, name: String) -> Result<std::collections::BTreeMap<String, u64>> {
    let (settings, _) = state.snapshot().await;
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    Ok(settings
        .sites
        .get(&name)
        .map(|own| {
            own.processes
                .iter()
                .filter_map(|p| {
                    let next = cron::Schedule::parse(p.schedule.trim()).ok()?.next(now)?;
                    Some((p.name.clone(), u64::try_from(next.unix_timestamp_nanos() / 1_000_000).ok()?))
                })
                .collect()
        })
        .unwrap_or_default())
}

/// Whether the project's database exists, and its server runs.
#[tauri::command]
async fn database_status(state: State<'_, AppState>, name: String) -> Result<Option<manage::DbStatus>> {
    let (settings, layout) = state.snapshot().await;
    let dir = project_dir(&layout, &name)?;
    let Some(database) = sites::scan(&settings, &layout).into_iter().find(|s| s.name == name).and_then(|s| s.database) else { return Ok(None) };
    let Some(engine) = manage::engine(&settings, &dir) else { return Ok(None) };
    let running = match engine.service() {
        Some(kind) => state.supervisor.is_running(kind).await,
        None => true,
    };
    Ok(manage::status(&settings, &layout, &dir, &database, running).await)
}

/// Creates the database the project's config names.
#[tauri::command]
async fn create_project_database(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<()> {
    let (settings, layout) = state.snapshot().await;
    let dir = project_dir(&layout, &name)?;
    let database = sites::scan(&settings, &layout).into_iter().find(|s| s.name == name).and_then(|s| s.database).ok_or_else(|| Error::msg(format!("{name} doesn't name a database in its config.")))?;
    let engine = manage::engine(&settings, &dir).ok_or_else(|| Error::msg("Install MariaDB, MySQL or PostgreSQL first (Services)."))?;
    if let Some(kind) = engine.service() {
        if !state.supervisor.is_running(kind).await {
            return Err(Error::msg(format!("Start {} first, then create the database.", engine.label())));
        }
    }
    manage::create(&settings, &layout, &dir, engine, &database).await?;
    logs::system(&app, logs::Level::Info, format!("Created database {database} in {} for {name}", engine.label()));
    Ok(())
}

/// Changes one of the .env settings the Overview shows (APP_URL…).
#[tauri::command]
async fn set_project_env(state: State<'_, AppState>, name: String, key: String, value: String) -> Result<()> {
    let (_, layout) = state.snapshot().await;
    manage::set_env_value(&project_dir(&layout, &name)?, &key, &value)
}

/// Which of the given ports something listens on.
#[tauri::command]
async fn ports_listening(ports: Vec<u16>) -> Result<Vec<u16>> {
    tauri::async_runtime::spawn_blocking(move || manage::listening(&ports)).await.map_err(|e| Error::msg(e.to_string()))
}

/// A site's own php.ini settings and Xdebug choice; PHP restarts with the web servers.
#[tauri::command]
async fn set_site_php_ini(app: AppHandle, state: State<'_, AppState>, name: String, values: std::collections::BTreeMap<String, String>, xdebug: bool) -> Result<()> {
    let _op = state.ops.lock().await;
    let (_, layout) = state.snapshot().await;
    project_dir(&layout, &name)?;
    for (key, value) in &values {
        if !configs::site_ini_ok(key, value) {
            return Err(Error::msg(format!("{key} = {value} isn't valid: use a size like 512M (or -1), whole seconds, or On/Off.")));
        }
    }
    state
        .update_settings(|s| {
            let own = s.sites.entry(name.clone()).or_default();
            own.php_ini = values;
            own.xdebug = xdebug;
        })
        .await?;
    logs::system(&app, logs::Level::Info, format!("Saved {name}'s PHP settings"));
    restart_web_servers(&app, &state).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectFileInfo {
    file: projectfile::ProjectFile,
    /// The project's setup is already what the file says.
    matches: bool,
}

/// The project's lamprabit.toml, if it has one, and whether its setup already matches it.
#[tauri::command]
async fn project_file(state: State<'_, AppState>, name: String) -> Result<Option<ProjectFileInfo>> {
    let (settings, layout) = state.snapshot().await;
    let dir = project_dir(&layout, &name)?;
    let Some(file) = projectfile::read(&dir)? else { return Ok(None) };
    let parent = format!("{}.{}", sites::label(&name).unwrap_or_default(), settings.tld);
    let current = projectfile::ProjectFile::from_settings(&settings.sites.get(&name).cloned().unwrap_or_default(), &parent);
    Ok(Some(ProjectFileInfo { matches: current == file, file }))
}

/// Writes the project's setup to lamprabit.toml in its folder.
#[tauri::command]
async fn save_project_file(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<()> {
    let (settings, layout) = state.snapshot().await;
    let dir = project_dir(&layout, &name)?;
    let parent = format!("{}.{}", sites::label(&name).unwrap_or_default(), settings.tld);
    let file = projectfile::ProjectFile::from_settings(&settings.sites.get(&name).cloned().unwrap_or_default(), &parent);
    projectfile::write(&dir, &file)?;
    logs::system(&app, logs::Level::Info, format!("Saved {name}'s setup to {}", projectfile::FILE));
    Ok(())
}

/// Stops everything running for a project: its app, processes, command and tunnels.
async fn stop_project(app: &AppHandle, state: &AppState, name: &str) {
    let (settings, _) = state.snapshot().await;
    state.apps.stop_processes(app, &settings, name, false).await;
    state.apps.stop_command(name).await;
    state.apps.stop(app, name).await;
    for tunnel in state.tunnels.list().await.into_iter().filter(|t| t.site.as_deref() == Some(name)) {
        state.tunnels.stop(app, &tunnel.id).await;
    }
}

/// Renames a project's folder, moving its settings and subdomains along. Its database
/// keeps its name (MySQL can't rename one); the .env still points at it.
#[tauri::command]
async fn rename_project(app: AppHandle, state: State<'_, AppState>, name: String, new_name: String) -> Result<()> {
    let _op = state.ops.lock().await;
    let (settings, layout) = state.snapshot().await;
    let dir = project_dir(&layout, &name)?;
    let new_name = new_name.trim().to_string();
    if new_name == name {
        return Ok(());
    }
    // Only the case changes: Windows sees the same folder, so skip the "taken" check.
    if !new_name.eq_ignore_ascii_case(&name) {
        manage::check_project_name(&layout, &new_name)?;
    }
    stop_project(&app, &state, &name).await;
    let target = layout.www().join(&new_name);
    std::fs::rename(&dir, &target).map_err(|e| Error::msg(format!("Couldn't rename {name}: {e}. Close editors, terminals and anything else using its folder, then try again.")))?;
    let old_suffix = format!(".{}.{}", sites::label(&name).unwrap_or_default(), settings.tld);
    let new_suffix = format!(".{}.{}", sites::label(&new_name).unwrap_or_default(), settings.tld);
    state
        .update_settings(|s| {
            if let Some(mut own) = s.sites.remove(&name) {
                for d in &mut own.domains {
                    if let Some(sub) = d.host.strip_suffix(&old_suffix) {
                        d.host = format!("{sub}{new_suffix}");
                    }
                }
                s.sites.insert(new_name.clone(), own);
            }
            // Profiles name processes as "project/process".
            for profile in &mut s.profiles {
                for key in &mut profile.processes {
                    if let Some(process) = key.strip_prefix(&format!("{name}/")) {
                        *key = format!("{new_name}/{process}");
                    }
                }
            }
        })
        .await?;
    logs::system(&app, logs::Level::Info, format!("Renamed {name} to {new_name}"));
    sync_sites(&app, &state).await
}

/// Deletes a project: stops it, drops its database if asked, and moves its folder to the
/// Recycle Bin (Trash), so it can still be brought back.
#[tauri::command]
async fn delete_project(app: AppHandle, state: State<'_, AppState>, name: String, drop_database: bool) -> Result<()> {
    let _op = state.ops.lock().await;
    let (settings, layout) = state.snapshot().await;
    let dir = project_dir(&layout, &name)?;
    stop_project(&app, &state, &name).await;
    if drop_database {
        let database = sites::scan(&settings, &layout).into_iter().find(|s| s.name == name).and_then(|s| s.database);
        if let (Some(database), Some(engine)) = (database, manage::engine(&settings, &dir)) {
            if let Some(kind) = engine.service() {
                if !state.supervisor.is_running(kind).await {
                    return Err(Error::msg(format!("Start {} to drop {database}, or delete the project without its database.", engine.label())));
                }
            }
            manage::drop(&settings, &layout, engine, &database).await?;
            logs::system(&app, logs::Level::Info, format!("Dropped database {database}"));
        }
    }
    let folder = dir.clone();
    tauri::async_runtime::spawn_blocking(move || trash::delete(&folder))
        .await
        .map_err(|e| Error::msg(e.to_string()))?
        .map_err(|e| Error::msg(format!("Couldn't move {name} to the Recycle Bin: {e}. Close editors, terminals and anything else using its folder, then try again.")))?;
    state.update_settings(|s| {
        s.sites.remove(&name);
        for profile in &mut s.profiles {
            profile.processes.retain(|k| !k.starts_with(&format!("{name}/")));
        }
    })
    .await?;
    logs::system(&app, logs::Level::Info, format!("Deleted {name} (moved to the Recycle Bin)"));
    sync_sites(&app, &state).await
}

#[tauri::command]
async fn stop_project_command(state: State<'_, AppState>, name: String) -> Result<()> {
    state.apps.stop_command(&name).await;
    Ok(())
}

/// Saves the commands the user added to a project.
#[tauri::command]
async fn save_project_commands(state: State<'_, AppState>, name: String, commands: Vec<commands::UserCommand>) -> Result<()> {
    let (_, layout) = state.snapshot().await;
    project_dir(&layout, &name)?;
    let cleaned: Vec<commands::UserCommand> = commands.into_iter().filter(|c| !c.name.trim().is_empty() && !c.run.trim().is_empty()).collect();
    state.update_settings(|s| s.sites.entry(name.clone()).or_default().commands = cleaned).await?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectFacts {
    #[serde(flatten)]
    facts: facts::Facts,
    /// The template it was created from (its name), when LampRabit created it.
    template: Option<String>,
}

/// Everything the Overview shows about a project beyond what the site list has.
#[tauri::command]
async fn project_facts(state: State<'_, AppState>, name: String) -> Result<ProjectFacts> {
    let (settings, layout) = state.snapshot().await;
    let dir = project_dir(&layout, &name)?;
    let site = sites::scan(&settings, &layout).into_iter().find(|s| s.name == name);
    let own = settings.sites.get(&name);
    let pm = own.and_then(|s| s.app.as_ref()).and_then(|a| a.package_manager).unwrap_or_else(|| apps::detect_package_manager(&dir));
    let template = own.and_then(|s| s.template.clone()).map(|id| {
        let id = id.split('@').next().unwrap_or(&id).to_string();
        templates::find(&layout, &id).map(|(t, _)| t.name).unwrap_or(id)
    });
    Ok(ProjectFacts { facts: facts::read(&dir, site.and_then(|s| s.framework), pm.id()).await, template })
}

/// A free port from 3000 up that no other project uses.
#[tauri::command]
async fn app_free_port(state: State<'_, AppState>) -> Result<u16> {
    Ok(apps::free_port(&*state.settings.read().await))
}

/// Shares a site (by folder name) or a local port on a public URL.
#[tauri::command]
async fn tunnel_start(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: tunnels::Provider,
    site: Option<String>,
    port: Option<u16>,
) -> Result<tunnels::Tunnel> {
    let (settings, layout) = state.snapshot().await;
    let site = match site {
        Some(name) => Some(
            sites::served(&settings, &layout)
                .into_iter()
                .find(|s| s.name == name)
                .ok_or_else(|| Error::msg(format!("No site called {name}")))?,
        ),
        None => None,
    };
    if let Some(s) = &site {
        let kind = server_kind(s.server);
        if !state.supervisor.is_running(kind).await {
            return Err(Error::msg(format!("Start {} first; the tunnel forwards to it.", kind.label())));
        }
    }
    state.tunnels.start(&app, &settings, &layout, provider, site.as_ref(), port).await
}

#[tauri::command]
async fn tunnel_stop(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    state.tunnels.stop(&app, &id).await;
    Ok(())
}

#[tauri::command]
async fn tunnel_list(state: State<'_, AppState>) -> Result<Vec<tunnels::Tunnel>> {
    Ok(state.tunnels.list().await)
}

#[tauri::command]
fn ngrok_token_saved() -> bool {
    tunnels::ngrok_token().is_some()
}

/// Saves the Ngrok authtoken in the system keychain (None removes it).
#[tauri::command]
fn set_ngrok_token(app: AppHandle, token: Option<String>) -> Result<()> {
    tunnels::set_ngrok_token(token.as_deref())?;
    logs::system(&app, logs::Level::Info, if token.is_some() { "Ngrok authtoken saved to the keychain" } else { "Ngrok authtoken removed" });
    Ok(())
}

#[tauri::command]
async fn node_status(app: AppHandle) -> Result<node::NodeStatus> {
    Ok(node::status(&app).await)
}

#[tauri::command]
async fn node_available(state: State<'_, AppState>) -> Result<Vec<node::NodeRelease>> {
    node::available(&state.http).await
}

#[tauri::command]
async fn node_install(app: AppHandle, version: String) -> Result<()> {
    node::install_version(&app, &version).await
}

#[tauri::command]
async fn node_use(app: AppHandle, version: String) -> Result<()> {
    node::use_version(&app, &version).await
}

#[tauri::command]
async fn node_uninstall(app: AppHandle, version: String) -> Result<()> {
    node::uninstall_version(&app, &version).await
}

#[tauri::command]
async fn nvm_install(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let (_, layout) = state.snapshot().await;
    node::install_nvm(&app, &state.http, &layout).await
}

#[tauri::command]
async fn runtime_updates(state: State<'_, AppState>) -> Result<Vec<runtimes::UpdateNotice>> {
    let (_, layout) = state.snapshot().await;
    Ok(runtimes::updates(&state.http, &layout).await)
}

#[tauri::command]
async fn open_terminal(state: State<'_, AppState>, dir: Option<PathBuf>) -> Result<()> {
    let (settings, layout) = state.snapshot().await;
    comfort::open_terminal(&settings, &layout, &dir.unwrap_or_else(|| layout.www()))
}

#[tauri::command]
async fn open_in_editor(state: State<'_, AppState>, dir: PathBuf) -> Result<()> {
    let (settings, _) = state.snapshot().await;
    comfort::open_in_editor(&settings, &dir).await
}

#[tauri::command]
fn vc_runtime_status() -> comfort::VcRuntime {
    comfort::vc_runtime()
}

#[tauri::command]
async fn vc_runtime_install(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let (_, layout) = state.snapshot().await;
    logs::system(&app, logs::Level::Info, "Installing the Microsoft Visual C++ runtime");
    comfort::install_vc_runtime(&state.http, &layout).await?;
    logs::system(&app, logs::Level::Info, "Microsoft Visual C++ runtime installed");
    Ok(())
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::{
        menu::{Menu, MenuItem, PredefinedMenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", "Open LampRabit", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "start", "Start all", true, None::<&str>)?,
            &MenuItem::with_id(app, "stop", "Stop all", true, None::<&str>)?,
            &MenuItem::with_id(app, "www", "Open www folder", true, None::<&str>)?,
            &MenuItem::with_id(app, "terminal", "Open terminal", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "Quit LampRabit", true, None::<&str>)?,
        ],
    )?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("LampRabit")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let app = app.clone();
            match event.id.as_ref() {
                "open" => show_main(&app),
                "quit" => app.exit(0),
                id => {
                    let id = id.to_string();
                    tauri::async_runtime::spawn(async move {
                        let state = app.state::<AppState>();
                        match id.as_str() {
                            "start" => {
                                let _ = start_all(app.clone(), state).await;
                            }
                            "stop" => {
                                let _ = stop_all(app.clone(), state).await;
                            }
                            "www" => {
                                let (_, layout) = state.snapshot().await;
                                use tauri_plugin_opener::OpenerExt;
                                let _ = app.opener().open_path(layout.www().to_string_lossy(), None::<&str>);
                            }
                            "terminal" => {
                                let _ = open_terminal(state, None).await;
                            }
                            _ => {}
                        }
                    });
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[tauri::command]
async fn php_extensions(state: State<'_, AppState>, version: String) -> Result<Vec<extensions::ExtInfo>> {
    let (settings, layout) = state.snapshot().await;
    extensions::list(&state.http, &settings, &layout, &version).await
}

/// Switches an extension on or off for one PHP version and restarts PHP to apply it.
#[tauri::command]
async fn set_php_extension(app: AppHandle, state: State<'_, AppState>, version: String, name: String, enabled: bool) -> Result<()> {
    let _op = state.ops.lock().await;
    state
        .update_settings(|s| {
            let choices = s.php_extensions.entry(version.clone()).or_default();
            if enabled {
                choices.disabled.remove(&name);
                if !extensions::DEFAULTS.contains(&name.as_str()) {
                    choices.enabled.insert(name.clone());
                }
            } else {
                choices.enabled.remove(&name);
                if extensions::DEFAULTS.contains(&name.as_str()) {
                    choices.disabled.insert(name.clone());
                }
            }
            // Only differences from the defaults are stored; back to defaults means no entry.
            if choices.enabled.is_empty() && choices.disabled.is_empty() {
                s.php_extensions.remove(&version);
            }
        })
        .await?;
    logs::system(&app, logs::Level::Info, format!("PHP {version}: {name} {}", if enabled { "enabled" } else { "disabled" }));
    restart_web_servers(&app, &state).await
}

/// Downloads an extra extension for one PHP version and enables it.
#[tauri::command]
async fn install_php_extension(app: AppHandle, state: State<'_, AppState>, version: String, name: String) -> Result<()> {
    let (_, layout) = state.snapshot().await;
    logs::system(&app, logs::Level::Info, format!("Installing {name} for PHP {version}"));
    let ext_version = extensions::install(&state.http, &layout, &version, &name).await?;
    logs::system(&app, logs::Level::Info, format!("Installed {name} {ext_version} for PHP {version}"));
    set_php_extension(app, state, version, name, true).await
}

#[tauri::command]
async fn php_status(state: State<'_, AppState>) -> Result<Vec<php::PoolStatus>> {
    Ok(state.supervisor.php.status().await)
}

/// Adds the local CA to the user's trust store (the OS asks for confirmation).
#[tauri::command]
async fn trust_ca(app: AppHandle, state: State<'_, AppState>) -> Result<Option<String>> {
    let (_, layout) = state.snapshot().await;
    *state.ca_trusted.lock().await = None;
    match ssl::trust_ca(&layout).await {
        Ok(note) => {
            logs::system(&app, logs::Level::Info, "Local certificate authority trusted");
            if let Some(note) = &note {
                logs::system(&app, logs::Level::Warn, note.clone());
            }
            Ok(note)
        }
        Err(e) => {
            logs::system(&app, logs::Level::Warn, e.to_string());
            Err(e)
        }
    }
}

/// Opens a database's command-line client in a new terminal window (the default
/// database when none is named).
#[tauri::command]
async fn open_db_client(state: State<'_, AppState>, database: Option<config::Database>) -> Result<()> {
    let (settings, layout) = state.snapshot().await;
    let db = database.or(settings.database()).ok_or_else(|| Error::msg("Install MariaDB or MySQL first (Services)."))?;
    let home = configs::database_home(&settings, &layout, db)?;
    let client = configs::database_client(&settings, &layout, db)?;
    let defaults = format!("--defaults-file={}", configs::write_client_config(&settings, &layout, db)?.display());
    if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", "start", db.label()])
            .arg(&client)
            .arg(defaults.as_str())
            .current_dir(&home)
            .spawn()?;
        Ok(())
    } else if cfg!(target_os = "macos") {
        Err(Error::msg(format!("Opening a terminal is not supported on this OS yet. Run: {} {defaults}", client.display())))
    } else {
        #[cfg(not(windows))]
        comfort::run_in_terminal(&home, &[client.to_string_lossy().into_owned(), defaults])?;
        Ok(())
    }
}

#[tauri::command]
async fn log_history(app: AppHandle, state: State<'_, AppState>) -> Result<logs::History> {
    let (_, layout) = state.snapshot().await;
    Ok(logs::history(&app, &layout.logs()))
}

/// The dumps caught this session, oldest first.
#[tauri::command]
fn list_dumps(app: AppHandle) -> Vec<dumps::Dump> {
    app.state::<dumps::DumpStore>().all()
}

/// Turns the dump catcher on or off; PHP restarts with the web servers to pick it up.
#[tauri::command]
async fn set_catch_dumps(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> Result<()> {
    let _op = state.ops.lock().await;
    state.update_settings(|s| s.catch_dumps = enabled).await?;
    logs::system(&app, logs::Level::Info, if enabled { "Catching dump() and dd() in Logs" } else { "dump() and dd() show in the page again" });
    restart_web_servers(&app, &state).await
}

/// Keeps the dump listener in step with the settings: listening on `ports.dumps` while
/// "Catch dumps" is on, closed otherwise.
fn spawn_dump_listener(handle: &AppHandle) {
    let handle = handle.clone();
    tauri::async_runtime::spawn(async move {
        let mut current: Option<(u16, tauri::async_runtime::JoinHandle<()>)> = None;
        loop {
            let (settings, layout) = handle.state::<AppState>().snapshot().await;
            let wanted = settings.catch_dumps.then_some(settings.ports.dumps);
            let finished = current.as_ref().is_some_and(|(_, task)| task.inner().is_finished());
            if current.as_ref().map(|(p, _)| *p) != wanted || finished {
                if let Some((_, task)) = current.take() {
                    task.abort();
                }
                if let Some(port) = wanted {
                    let (app, www) = (handle.clone(), layout.www());
                    let task = tauri::async_runtime::spawn(async move {
                        if let Err(e) = dumps::serve(app.clone(), port, www).await {
                            logs::system(&app, logs::Level::Warn, format!("Can't catch dumps on port {port}: {e}. Change the dumps port in Settings → Ports."));
                        }
                    });
                    current = Some((port, task));
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(if finished { 30 } else { 2 })).await;
        }
    });
}

/// Empties the log files of one source ("apache", "dumps", "project:shop"), or all of them.
#[tauri::command]
async fn clear_logs(app: AppHandle, state: State<'_, AppState>, source: Option<String>) -> Result<()> {
    let (_, layout) = state.snapshot().await;
    let failed = logs::clear(&app, &layout.logs(), source.as_deref());
    if source.is_none() || source.as_deref() == Some("dumps") {
        app.state::<dumps::DumpStore>().clear();
    }
    logs::system(&app, logs::Level::Info, match &source {
        Some(s) => format!("Logs of {} cleared", s.strip_prefix("project:").unwrap_or(s)),
        None => "Logs cleared".to_string(),
    });
    if failed.is_empty() {
        Ok(())
    } else {
        Err(Error::msg(format!("Could not clear {} (in use by a running service?)", failed.join(", "))))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PortStatus {
    /// Settings key, e.g. "http" or "mailSmtp".
    key: &'static str,
    port: u16,
    free: bool,
    /// Set when one of LampRabit's own services holds the port.
    used_by_service: Option<&'static str>,
    owner: Option<diagnose::PortOwner>,
}

/// Who, if anyone, is on each configured port.
#[tauri::command]
async fn port_status(state: State<'_, AppState>) -> Result<Vec<PortStatus>> {
    let (settings, layout) = state.snapshot().await;
    let p = &settings.ports;
    let mut ports = vec![("http", p.http), ("https", p.https), ("mysql", p.mysql), ("mysqlServer", p.mysql_server), ("mailSmtp", p.mail_smtp), ("mailUi", p.mail_ui), ("phpFpm", p.php_fpm), ("redis", p.redis), ("memcached", p.memcached), ("postgres", p.postgres), ("pgadmin", p.pgadmin), ("mongodb", p.mongodb), ("clickhouseHttp", p.clickhouse_http), ("clickhouseTcp", p.clickhouse_tcp), ("neo4jHttp", p.neo4j_http), ("neo4jBolt", p.neo4j_bolt), ("meilisearch", p.meilisearch), ("typesense", p.typesense), ("rustfs", p.rustfs), ("rustfsConsole", p.rustfs_console), ("ollama", p.ollama), ("minio", p.minio), ("minioConsole", p.minio_console), ("elasticsearch", p.elasticsearch), ("elasticsearchTransport", p.elasticsearch_transport), ("opensearch", p.opensearch), ("opensearchTransport", p.opensearch_transport), ("n8n", p.n8n), ("rabbitmq", p.rabbitmq), ("rabbitmqManagement", p.rabbitmq_management)];
    // Servers that aren't the primary one have their own pair of ports.
    for &server in settings.web_servers.iter().skip(1) {
        let (http, https) = settings.server_ports(server);
        ports.extend(match server {
            config::Server::Apache => [("apacheHttp", http), ("apacheHttps", https)],
            config::Server::Nginx => [("nginxHttp", http), ("nginxHttps", https)],
            config::Server::Caddy => [("caddyHttp", http), ("caddyHttps", https)],
            config::Server::Frankenphp => [("frankenphpHttp", http), ("frankenphpHttps", https)],
        });
    }
    if settings.server_enabled(config::Server::Caddy) {
        ports.push(("caddyAdmin", p.caddy_admin));
    }
    if settings.server_enabled(config::Server::Frankenphp) {
        ports.push(("frankenphpAdmin", p.frankenphp_admin));
    }
    if settings.versions.get("tomcat").is_some() {
        ports.extend([("tomcatHttp", p.tomcat_http), ("tomcatShutdown", p.tomcat_shutdown)]);
    }
    if settings.catch_dumps {
        ports.push(("dumps", p.dumps));
    }
    let mut out = Vec::new();
    for (key, port) in ports {
        let free = services::port_free(port);
        let mut used_by_service = None;
        for kind in ServiceKind::all() {
            if state.supervisor.is_running(kind).await && services::required_ports(kind, &settings, &layout).contains(&port) {
                used_by_service = Some(kind.label());
            }
        }
        if state.supervisor.php.status().await.iter().any(|p| (p.port..p.port + p.workers).contains(&port)) {
            used_by_service = Some("PHP");
        }
        let owner = if free || used_by_service.is_some() { None } else { diagnose::port_owner(port).await };
        out.push(PortStatus { key, port, free, used_by_service, owner });
    }
    Ok(out)
}

/// Stops a process that is blocking a port. Refuses system processes and LampRabit's own.
#[tauri::command]
async fn stop_process(app: AppHandle, state: State<'_, AppState>, pid: u32) -> Result<()> {
    if pid == 0 || pid == 4 || pid == std::process::id() || state.supervisor.pids().await.contains_key(&pid) {
        return Err(Error::msg("LampRabit won't stop that process."));
    }
    let status = if cfg!(windows) {
        let mut cmd = tokio::process::Command::new("taskkill");
        cmd.args(["/PID", &pid.to_string(), "/F"]);
        procgroup::prepare(&mut cmd);
        cmd.status().await?
    } else {
        tokio::process::Command::new("kill").arg(pid.to_string()).status().await?
    };
    if !status.success() {
        return Err(Error::msg(format!(
            "Couldn't stop process {pid}. It may belong to another user or need administrator rights."
        )));
    }
    logs::system(&app, logs::Level::Info, format!("Stopped process {pid} to free its port"));
    Ok(())
}

#[tauri::command]
fn clear_problem(state: State<'_, AppState>, kind: ServiceKind) {
    state.supervisor.clear_problem(kind);
}

#[tauri::command]
fn save_text(path: PathBuf, contents: String) -> Result<()> {
    std::fs::write(path, contents)?;
    Ok(())
}

/// A Markdown report of everything useful for troubleshooting.
#[tauri::command]
async fn diagnostics(state: State<'_, AppState>, redact: bool) -> Result<String> {
    use std::fmt::Write;
    let (settings, layout) = state.snapshot().await;
    let mut r = String::new();
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());

    let _ = writeln!(r, "# LampRabit diagnostics\n");
    let generated = now
        .format(time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"))
        .unwrap_or_default();
    let _ = writeln!(r, "Generated {generated}\n");
    let _ = writeln!(r, "## System\n");
    let _ = writeln!(r, "- LampRabit {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(r, "- OS: {} {} ({})", std::env::consts::OS, os_version().await, std::env::consts::ARCH);
    if cfg!(windows) {
        let sys = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into())).join("System32");
        for dll in ["vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll"] {
            let found = if sys.join(dll).exists() { "found" } else { "MISSING (install the VC++ 2015–2022 Redistributable)" };
            let _ = writeln!(r, "- {dll}: {found}");
        }
    }

    let _ = writeln!(r, "\n## Environment\n");
    let _ = writeln!(r, "- Root: {}", layout.root().display());
    let _ = writeln!(r, "- Domain suffix: .{}", settings.tld);
    let servers: Vec<&str> = settings.web_servers.iter().map(|s| s.label()).collect();
    let _ = writeln!(r, "- Web servers: {}", servers.join(", "));
    let _ = writeln!(r, "- HTTPS by default: {}", settings.https_by_default);
    let _ = writeln!(r, "- Manage hosts file: {}", settings.manage_hosts);
    let _ = writeln!(r, "- PHP mail() to Mailpit: {}", settings.route_mail);

    let _ = writeln!(r, "\n## Runtimes\n\n| Runtime | Active | Installed |\n|---|---|---|");
    for runtime in Runtime::ALL {
        let active = settings.versions.get(runtime.id()).cloned().unwrap_or_else(|| "—".into());
        let installed = runtimes::installed(&layout, runtime).join(", ");
        let _ = writeln!(r, "| {} | {active} | {} |", runtime.id(), if installed.is_empty() { "—".into() } else { installed });
    }

    let _ = writeln!(r, "\n## Services\n\n| Service | State | PID | Port | Health | Last problem |\n|---|---|---|---|---|---|");
    for s in state.supervisor.list(&settings, &layout).await {
        let health = s.health.as_ref().map_or("—".to_string(), |h| match &h.detail {
            Some(d) => format!("{} ({d})", h.status),
            None => h.status.to_string(),
        });
        let problem = s.problem.as_ref().map_or("—".to_string(), |p| format!("{}: {}", p.title, p.detail));
        let _ = writeln!(
            r,
            "| {} {} | {:?} | {} | {} | {health} | {} |",
            s.label,
            s.version.as_deref().unwrap_or("(not installed)"),
            s.state,
            s.pid.map_or("—".into(), |p| p.to_string()),
            s.port,
            problem.replace('|', "\\|"),
        );
    }

    let _ = writeln!(r, "\n## Ports\n\n| Setting | Port | Status |\n|---|---|---|");
    for p in port_status(state.clone()).await? {
        let status = if p.free {
            "free".to_string()
        } else if let Some(svc) = p.used_by_service {
            format!("in use by LampRabit ({svc})")
        } else if let Some(o) = &p.owner {
            format!("in use by {} (PID {})", o.name, o.pid)
        } else {
            "in use (owner unknown)".into()
        };
        let _ = writeln!(r, "| {} | {} | {status} |", p.key, p.port);
    }

    let _ = writeln!(r, "\n## PHP workers\n");
    let pools = state.supervisor.php.status().await;
    if pools.is_empty() {
        let _ = writeln!(r, "None running (they start with a web server).");
    }
    for p in pools {
        let _ = writeln!(r, "- PHP {}: {}/{} workers alive, ports {}-{}", p.version, p.alive, p.workers, p.port, p.port + p.workers - 1);
    }

    for kind in ServiceKind::WEB {
        let _ = writeln!(r, "\n## {} configuration test\n", kind.label());
        let _ = match services::config_test(kind, &settings, &layout).await {
            None => writeln!(r, "Not run (not installed or never started)."),
            Some(Ok(())) => writeln!(r, "Syntax OK"),
            Some(Err(e)) => writeln!(r, "```text\n{e}\n```"),
        };
    }

    let served = sites::served(&settings, &layout);
    let _ = writeln!(r, "\n## Sites and HTTPS\n");
    let _ = writeln!(r, "- Sites: {}", served.iter().map(|s| s.domain.as_str()).collect::<Vec<_>>().join(", "));
    let _ = writeln!(r, "- Hosts file in sync: {}", !sites::needs_hosts_file(&settings) || sites::hosts_in_sync(&served));
    let _ = writeln!(r, "- Local CA: {}", if ssl::ca_thumbprint(&layout).is_some() {
        if ssl::ca_trusted(&layout).await { "created, trusted" } else { "created, NOT trusted" }
    } else {
        "not created"
    });

    let _ = writeln!(r, "\n## Recent warnings and errors\n");
    for file in ["lampRabit.log", "apache-error.log", "mariadb-error.log", "php-error.log", "mailpit-output.log"] {
        let tail = diagnose::tail(&layout.logs().join(file), 64 * 1024);
        let lines: Vec<&str> = tail
            .lines()
            .filter(|l| {
                let l = l.to_lowercase();
                ["error", "warn", "fatal", "crit", "emerg"].iter().any(|k| l.contains(k))
            })
            .collect();
        let start = lines.len().saturating_sub(15);
        if !lines.is_empty() {
            let _ = writeln!(r, "### {file}\n\n```text\n{}\n```\n", lines[start..].join("\n"));
        }
    }

    if redact {
        if let Some(home) = dirs::home_dir() {
            let home = home.to_string_lossy().to_string();
            r = r.replace(&home, "~").replace(&home.replace('\\', "/"), "~");
        }
        for var in ["USERNAME", "USER"] {
            if let Some(user) = std::env::var_os(var).map(|u| u.to_string_lossy().to_string()).filter(|u| u.len() > 2) {
                r = r.replace(&user, "<user>");
            }
        }
    }
    Ok(r)
}

async fn os_version() -> String {
    let (program, args): (&str, &[&str]) = if cfg!(windows) {
        ("cmd", &["/C", "ver"])
    } else if cfg!(target_os = "macos") {
        ("sw_vers", &["-productVersion"])
    } else {
        ("uname", &["-r"])
    };
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args);
    procgroup::prepare(&mut cmd);
    cmd.output().await.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

#[tauri::command]
fn check_port(port: u16) -> bool {
    services::port_free(port)
}

/// Every runtime with its group, description, source, platform gaps and companions.
#[tauri::command]
fn runtime_info() -> catalog::Catalog {
    catalog::catalog()
}

/// What a runtime version needs, and how each need gets met (see `catalog::requirements`).
#[tauri::command]
async fn runtime_requirements(app: AppHandle, state: State<'_, AppState>, runtime: Runtime, version: Option<String>) -> Result<Vec<catalog::Requirement>> {
    let (settings, layout) = state.snapshot().await;
    catalog::requirements(&app, &state.http, &settings, &layout, runtime, version).await
}

#[tauri::command]
async fn runtime_catalog(state: State<'_, AppState>, runtime: Runtime, fresh: Option<bool>) -> Result<Vec<Release>> {
    runtimes::available(&state.http, runtime, fresh.unwrap_or(false)).await
}

#[tauri::command]
async fn installed_runtimes(state: State<'_, AppState>) -> Result<Vec<InstalledRuntime>> {
    let (settings, layout) = state.snapshot().await;
    Ok(Runtime::ALL
        .into_iter()
        .map(|runtime| InstalledRuntime {
            runtime,
            active: settings.versions.get(runtime.id()).cloned(),
            versions: runtimes::installed(&layout, runtime),
        })
        .collect())
}

#[tauri::command]
async fn install_runtime(app: AppHandle, state: State<'_, AppState>, release: Release) -> Result<()> {
    let key = (release.runtime, release.version.clone());
    if !state.installing.lock().await.insert(key.clone()) {
        return Err(Error::msg(format!("{} {} is already being installed", release.runtime.id(), release.version)));
    }
    let (_, layout) = state.snapshot().await;
    let label = format!("{} {}", release.runtime.id(), release.version);
    logs::system(&app, logs::Level::Info, format!("Installing {label} from {}", release.url));
    let result = async {
        runtimes::install(&app, &state.http, &layout, &release).await?;
        // Some runtimes need a setup step after the download; the install isn't finished
        // (and the version isn't used) until it is.
        let setup = match release.runtime {
            Runtime::Rust => {
                logs::system(&app, logs::Level::Info, format!("Installing Rust {} with rustup", release.version));
                runtimes::setup_rust(&layout, &release.version).await
            }
            Runtime::N8n => {
                logs::system(&app, logs::Level::Info, "Installing n8n and its packages with npm (several minutes)");
                runtimes::setup_n8n(&layout, &release.version).await
            }
            Runtime::Pgadmin => {
                logs::system(&app, logs::Level::Info, "Installing pgAdmin's Python packages (a few minutes)");
                let settings = state.snapshot().await.0;
                runtimes::setup_pgadmin(&app, &settings, &layout, &release.version).await
            }
            Runtime::Phpredisadmin => {
                logs::system(&app, logs::Level::Info, "Adding Predis, phpRedisAdmin's Redis client");
                runtimes::add_predis(&state.http, &layout, &release.version).await
            }
            _ => Ok(()),
        };
        if runtimes::has_setup(release.runtime) {
            let stage = if setup.is_ok() { "done" } else { "error" };
            let _ = tauri::Emitter::emit(&app, "install-progress", runtimes::InstallProgress { runtime: release.runtime, version: release.version.clone(), stage, downloaded: 0, total: None });
        }
        setup
    }
    .await;
    state.installing.lock().await.remove(&key);
    match &result {
        Ok(()) => logs::system(&app, logs::Level::Info, format!("Installed {label}")),
        Err(e) => logs::system(&app, logs::Level::Error, format!("Installing {label} failed: {e}")),
    }
    result?;

    // The first installed version of a runtime becomes the active one.
    let mut activated = false;
    state
        .update_settings(|s| {
            if s.versions.get(release.runtime.id()).is_none() {
                s.versions.set(release.runtime.id(), Some(release.version.clone()));
                activated = true;
            }
        })
        .await?;
    // A new tool brings a domain and, for Mailpit, PHP mail settings: the primary web
    // server picks both up on restart.
    if activated && is_tool(release.runtime) {
        let primary = server_kind(state.snapshot().await.0.primary_server());
        restart_if_running(&app, &state, primary).await?;
    }
    Ok(())
}

fn is_tool(runtime: Runtime) -> bool {
    matches!(runtime, Runtime::Mailpit | Runtime::Adminer | Runtime::Phpmyadmin | Runtime::Phpredisadmin | Runtime::Phppgadmin | Runtime::Pgadmin | Runtime::Neo4j | Runtime::N8n)
}

async fn restart_if_running(app: &AppHandle, state: &AppState, kind: ServiceKind) -> Result<()> {
    if !state.supervisor.is_running(kind).await {
        return Ok(());
    }
    state.supervisor.stop(app, kind).await;
    state.supervisor.wait_stopped(kind).await;
    let (settings, layout) = state.snapshot().await;
    state.supervisor.start(app, kind, &settings, &layout).await
}

#[tauri::command]
async fn remove_runtime(app: AppHandle, state: State<'_, AppState>, runtime: Runtime, version: String) -> Result<()> {
    let (settings, layout) = state.snapshot().await;
    let was_active = settings.versions.get(runtime.id()) == Some(&version);
    // Web tools are just files served by Apache, which restarts below instead of blocking.
    if was_active && !matches!(runtime, Runtime::Adminer | Runtime::Phpmyadmin | Runtime::Phpredisadmin | Runtime::Phppgadmin) {
        for &kind in dependents(runtime) {
            if state.supervisor.is_running(kind).await {
                return Err(Error::msg(format!("Stop {} before removing the version it is using", kind.label())));
            }
        }
    }
    // A site can run a PHP version other than the default; its workers lock the files.
    if runtime == Runtime::Php && state.supervisor.php.status().await.iter().any(|p| p.version == version && p.alive > 0) {
        let users: Vec<String> =
            settings.sites.iter().filter(|(_, s)| s.php.as_deref() == Some(version.as_str())).map(|(n, _)| n.clone()).collect();
        return Err(Error::msg(format!(
            "PHP {version} is running for {}. Switch {} to another version or stop the web server first.",
            if users.is_empty() { "a site".to_string() } else { users.join(", ") },
            if users.len() == 1 { "that site" } else { "those sites" },
        )));
    }
    runtimes::remove(&layout, runtime, &version)?;
    logs::system(&app, logs::Level::Info, format!("Removed {} {version}", runtime.id()));
    state
        .update_settings(|s| {
            reconcile_versions(s);
            // Sites pinned to the removed PHP go back to the default version.
            if runtime == Runtime::Php {
                for site in s.sites.values_mut().filter(|site| site.php.as_deref() == Some(version.as_str())) {
                    site.php = None;
                }
            }
        })
        .await?;
    if was_active && is_tool(runtime) {
        restart_if_running(&app, &state, server_kind(settings.primary_server())).await?;
    }
    Ok(())
}

/// Switches the active version, restarting services that were using the old one.
#[tauri::command]
async fn set_active_version(
    app: AppHandle,
    state: State<'_, AppState>,
    runtime: Runtime,
    version: String,
) -> Result<()> {
    let _op = state.ops.lock().await;
    let (_, layout) = state.snapshot().await;
    if !runtimes::installed(&layout, runtime).contains(&version) {
        return Err(Error::msg(format!("{} {version} is not installed", runtime.id())));
    }

    let mut restart = Vec::new();
    for &kind in dependents(runtime) {
        if state.supervisor.is_running(kind).await {
            state.supervisor.stop(&app, kind).await;
            restart.push(kind);
        }
    }
    for &kind in &restart {
        state.supervisor.wait_stopped(kind).await;
    }

    logs::system(&app, logs::Level::Info, format!("Switching {} to {version}", runtime.id()));
    let settings = state.update_settings(|s| s.versions.set(runtime.id(), Some(version))).await?;
    let layout = Layout::new(&settings.root_dir);
    for kind in restart {
        state.supervisor.start(&app, kind, &settings, &layout).await?;
    }
    Ok(())
}

/// Crashed services come back after a short pause when Settings says so; the pause lets
/// ports and lock files go first.
fn spawn_crash_restarts(app: &AppHandle) {
    let handle = app.clone();
    let crashes = app.state::<AppState>().supervisor.crashes();
    tauri::async_runtime::spawn(async move {
        let Some(mut crashes) = crashes else { return };
        while let Some(kind) = crashes.recv().await {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            let state = handle.state::<AppState>();
            let _op = state.ops.lock().await;
            let (settings, layout) = state.snapshot().await;
            // start() already logged and recorded why it failed.
            let _ = state.supervisor.restart_crashed(&handle, kind, &settings, &layout).await;
        }
    });
}

/// Project processes that exited and should run again (apps.rs decides; this has the
/// settings to start them with).
fn spawn_process_restarts(app: &AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(mut requests) = apps::restart_requests() else { return };
        while let Some((project, process)) = requests.recv().await {
            let state = handle.state::<AppState>();
            let (settings, layout) = state.snapshot().await;
            if let Err(e) = state.apps.start_process(&handle, &settings, &layout, &project, &process).await {
                let key = apps::process_key(&project, &process);
                state.apps.record(&handle, &layout, &key, "error", format!("Couldn't restart {process}: {e}")).await;
            }
        }
    });
}

/// `--smoke-test`: the services through the real supervisor, without a window or the
/// user's settings (smoke.rs). Exits with the test's result.
fn run_smoke_test(mut context: tauri::Context<tauri::Wry>, options: smoke::Options) {
    context.config_mut().app.windows.clear();
    let settings = smoke::settings(&options);
    if let Err(e) = Layout::new(&settings.root_dir).ensure() {
        eprintln!("smoke test: can't use {}: {e}", settings.root_dir.display());
        std::process::exit(2);
    }
    let settings_path = settings.root_dir.join("settings.toml");
    let _ = settings.save(&settings_path);
    let app = tauri::Builder::default()
        .setup(move |app| {
            let root = settings.root_dir.clone();
            app.manage(AppState::new(settings, settings_path));
            app.manage(logs::LogHub::default());
            logs::set_system_file(app.handle(), Layout::new(&root).logs().join("lampRabit.log"));
            spawn_crash_restarts(app.handle());
            spawn_process_restarts(app.handle());
            spawn_process_schedules(app.handle());
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let code = smoke::run(handle, options).await;
                // The services are stopped by now. AppHandle::exit loses the code on Linux
                // (GTK's loop ends with 0), and CI needs it.
                std::process::exit(code);
            });
            Ok(())
        })
        .build(context)
        .expect("error while building LampRabit");
    app.run(|_, _| {});
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // The Windows uninstaller's cleanup: no window, and before the single-instance plugin,
    // which would otherwise hand the call to a running copy.
    if std::env::args().any(|a| a == "--uninstall-cleanup") {
        uninstall::cli();
        return;
    }
    let context = tauri::generate_context!();
    // Also before the single-instance plugin: a LampRabit already open must not take it.
    if let Some(options) = smoke::options() {
        match options {
            Ok(options) => run_smoke_test(context, options),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(2);
            }
        }
        return;
    }
    tauri::Builder::default()
        // Must be registered first: a second launch hands over to the running copy and exits
        // before anything else (tray, services, log files) is set up.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .on_window_event(|window, event| {
            // Closing keeps LampRabit (and its services) running in the tray.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                let to_tray = tauri::async_runtime::block_on(async { state.settings.read().await.close_to_tray });
                if to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let settings_path = config_dir.join("settings.toml");
            // Pre-release builds used the identifier "dev.lamprabit.app"; carry their settings over.
            if !settings_path.exists() {
                if let Some(old) = config_dir.parent().map(|p| p.join("dev.lamprabit.app").join("settings.toml")) {
                    if old.exists() {
                        let _ = std::fs::create_dir_all(&config_dir);
                        let _ = std::fs::copy(&old, &settings_path);
                    }
                }
            }
            let mut warnings = Vec::new();
            let (mut settings, broken) = Settings::load_or_recover(&settings_path);
            // A first run: no settings yet, and no LampRabit folder already where they point
            // (a reinstall keeps using its folder without asking).
            let fresh = !settings_path.exists() && !["bin", "www", "data"].iter().any(|d| settings.root_dir.join(d).exists());
            app.manage(FirstRun(std::sync::atomic::AtomicBool::new(fresh)));
            if let Some(aside) = broken {
                warnings.push(format!(
                    "Your settings file was damaged, so LampRabit started with default settings. The old file was kept as {}.",
                    aside.display()
                ));
            }
            if let Err(e) = Layout::new(&settings.root_dir).ensure() {
                warnings.push(format!("LampRabit can't use its environment folder {}: {e}. Choose another one in Settings → General.", settings.root_dir.display()));
            }
            runtimes::clean_removed(&Layout::new(&settings.root_dir));
            runtimes::migrate_flat_composer(&Layout::new(&settings.root_dir));
            reconcile_versions(&mut settings);
            if tidy_extension_choices(&mut settings) {
                let _ = settings.save(&settings_path);
            }
            app.manage(AppState::new(settings, settings_path));
            app.manage(logs::LogHub::default());
            app.manage(dumps::DumpStore::default());
            spawn_dump_listener(app.handle());
            {
                let root = app.state::<AppState>().settings.try_read().map(|s| s.root_dir.clone()).unwrap_or_default();
                logs::set_system_file(app.handle(), Layout::new(root).logs().join("lampRabit.log"));
            }
            logs::system(app.handle(), logs::Level::Info, format!("LampRabit {} started on {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS));
            for w in &warnings {
                logs::system(app.handle(), logs::Level::Warn, w.clone());
            }
            {
                let root = app.state::<AppState>().settings.try_read().map(|s| s.root_dir.clone()).unwrap_or_default();
                let leftovers = services::reap_orphans(&Layout::new(&root));
                if !leftovers.is_empty() {
                    logs::system(
                        app.handle(),
                        logs::Level::Warn,
                        format!("Stopped services left running by an earlier session: {}", leftovers.join(", ")),
                    );
                }
                // After reaping, so a Garnet left running doesn't keep its files locked.
                if runtimes::remove_garnet(&Layout::new(&root)) {
                    warnings.push("Redis is now real Redis on every platform, replacing Garnet. Install it again from Services → Redis; phpRedisAdmin and redis-cli use it automatically.".into());
                }
            }
            app.manage(StartupWarnings(std::sync::Mutex::new(warnings)));
            if let Ok(settings) = app.state::<AppState>().settings.try_read() {
                refresh_shims(app.handle(), &settings);
            }

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    let state = handle.state::<AppState>();
                    let (settings, _) = state.snapshot().await;
                    state.supervisor.check_health(&handle, &settings, &state.http).await;
                }
            });

            spawn_crash_restarts(app.handle());
            spawn_process_restarts(app.handle());

            tauri::async_runtime::spawn(watch_ports(app.handle().clone()));

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut tailer = logs::Tailer::default();
                let mut polls: u32 = 0;
                loop {
                    let settings = handle.state::<AppState>().settings.read().await.clone();
                    let dir = Layout::new(&settings.root_dir).logs();
                    tailer.poll(&handle, &dir);
                    // About once a minute: keep log files to their size (Settings → Logs).
                    polls = polls.wrapping_add(1);
                    if polls % 150 == 1 {
                        let mb = |n: u32| u64::from(n.max(1)) * 1024 * 1024;
                        let (max, keep, total) = (mb(settings.log_max_mb), settings.log_keep, mb(settings.log_total_mb));
                        let dir2 = dir.clone();
                        let rotated = tauri::async_runtime::spawn_blocking(move || logs::rotate(&dir2, max, keep, total)).await.unwrap_or(0);
                        if rotated > 0 {
                            logs::system(&handle, logs::Level::Info, format!("Rotated {rotated} log file(s) over {} MB", settings.log_max_mb));
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                }
            });

            setup_tray(app)?;
            // Started with the OS: stay in the tray.
            if std::env::args().any(|a| a == "--minimized") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<AppState>();
                if state.settings.read().await.start_services_on_launch {
                    logs::system(&handle, logs::Level::Info, "Starting services on launch");
                    let _ = start_all(handle.clone(), state).await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            runtime_info,
            set_site_worker,
            set_site_domains,
            set_site_main,
            project_folders,
            set_site_processes,
            first_run,
            finish_first_run,
            template_from_project,
            save_template,
            list_dumps,
            set_catch_dumps,
            apply_processes,
            project_file,
            save_project_file,
            database_status,
            create_project_database,
            set_project_env,
            ports_listening,
            set_site_php_ini,
            rename_project,
            delete_project,
            command_history,
            process_next_runs,
            process_start,
            process_stop,
            process_restart,
            processes_all,
            restart_frankenphp_workers,
            runtime_requirements,
            get_settings,
            save_settings,
            env_paths,
            list_services,
            start_service,
            stop_service,
            start_all,
            stop_all,
            uninstall_plan,
            uninstall,
            list_templates,
            open_templates_folder,
            app_info,
            app_save,
            app_start,
            app_stop,
            app_restart,
            app_install,
            app_statuses,
            app_logs,
            app_free_port,
            project_facts,
            set_site_route,
            project_commands,
            run_project_command,
            stop_project_command,
            save_project_commands,
            apply_services,
            service_usage,
            check_port,
            port_status,
            stop_process,
            clear_problem,
            diagnostics,
            save_text,
            log_history,
            clear_logs,
            restart_service,
            list_sites,
            apply_sites,
            set_site_ssl,
            set_site_php,
            set_site_server,
            php_status,
            create_project,
            php_extensions,
            set_php_extension,
            install_php_extension,
            open_terminal,
            take_startup_warnings,
            open_in_editor,
            vc_runtime_status,
            vc_runtime_install,
            runtime_updates,
            node_status,
            node_available,
            node_install,
            node_use,
            node_uninstall,
            nvm_install,
            tunnel_start,
            tunnel_stop,
            tunnel_list,
            ngrok_token_saved,
            set_ngrok_token,
            trust_ca,
            list_databases,
            list_backups,
            backup_database,
            restore_backup,
            delete_backup,
            open_compass,
            check_move_target,
            environment_in_use,
            stop_environment_holder,
            move_environment,
            ollama_elsewhere,
            set_language_version,
            open_db_client,
            runtime_catalog,
            installed_runtimes,
            install_runtime,
            remove_runtime,
            set_active_version,
        ])
        .build(context)
        .expect("error while building LampRabit")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                let state = app.state::<AppState>();
                tauri::async_runtime::block_on(async {
                    state.tunnels.stop_all().await;
                    state.apps.stop_all().await;
                    state.supervisor.shutdown().await;
                });
            }
        });
}
