//! What the Services page offers, from the backend: each runtime's group, label, what it's
//! for and where it comes from, where it can't run and why, and what it needs (with the
//! versions that work) or goes well with. The frontend builds its lists from this, so a new
//! runtime shows up without frontend tables; only its icon lives there.

use serde::Serialize;
use tauri::AppHandle;

use crate::{
    config::Settings,
    error::Result,
    layout::Layout,
    runtimes::{self, Release, Runtime},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: &'static str,
    pub title: &'static str,
    pub blurb: &'static str,
    /// Runtime ids, plus "node" (NVM's versions) and "tunnels" (the tunnel list).
    pub items: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub blurb: &'static str,
    /// Where the download comes from, on this OS.
    pub source: &'static str,
    /// Why it can't be installed on this OS or machine, when it can't.
    pub unavailable: Option<&'static str>,
    /// Ids of what it needs; the versions depend on its own version (`requirements`).
    pub needs: Vec<&'static str>,
    /// Ids of what goes well with it: a UI for its data, its shell.
    pub suggests: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub groups: Vec<Group>,
    pub runtimes: Vec<RuntimeInfo>,
}

/// Services by what they do, in the order the Services page shows them.
fn groups() -> Vec<Group> {
    let g = |id, title, blurb, items: &[&'static str]| Group { id, title, blurb, items: items.to_vec() };
    vec![
        g("web", "Web and app servers", "Apache, Nginx, Caddy or FrankenPHP serve your sites; Tomcat runs Java web apps behind them. Several can run side by side.", &["apache", "nginx", "caddy", "frankenphp", "tomcat"]),
        g("languages", "Languages", "Several versions side by side; projects pick theirs.", &["php", "java", "python", "go", "rust", "node"]),
        g("packages", "Package managers", "Run them from LampRabit's terminal, or anywhere once LampRabit's commands are on PATH.", &["composer", "maven", "gradle"]),
        g("databases", "Databases", "They can all run side by side, each on its own port.", &["mariadb", "mysql", "postgres", "mongodb", "clickhouse", "neo4j", "sqlite"]),
        g("search", "Search", "Full-text search engines for your apps.", &["meilisearch", "typesense", "elasticsearch", "opensearch"]),
        g("storage", "Storage", "S3-compatible object storage. Their storage folder can be anywhere, even outside the LampRabit folder.", &["rustfs", "minio"]),
        g("queues", "Queues", "Message brokers for background jobs and events.", &["rabbitmq", "erlang"]),
        g("ai", "AI and automation", "Run language models locally and automate workflows.", &["ollama", "n8n"]),
        g("cache", "Cache", "In-memory stores for caches, sessions and queues.", &["redis", "memcached"]),
        g("tools", "Developer tools", "Mail catching and web interfaces for your databases and caches.", &["mailpit", "phpmyadmin", "adminer", "phppgadmin", "pgadmin", "phpredisadmin", "compass", "mongosh", "mongotools"]),
        g("sharing", "Sharing", "Put a project or local port on a public URL.", &["cloudflared", "ngrok", "tunnels"]),
    ]
}

pub fn catalog() -> Catalog {
    let mut groups = groups();
    // A runtime nobody put in a group still shows, under Other.
    let grouped: Vec<&str> = groups.iter().flat_map(|g| g.items.clone()).collect();
    let other: Vec<&'static str> = Runtime::ALL.iter().map(|r| r.id()).filter(|id| !grouped.contains(id)).collect();
    if !other.is_empty() {
        groups.push(Group { id: "other", title: "Other", blurb: "More services.", items: other });
    }
    Catalog { groups, runtimes: Runtime::ALL.into_iter().map(info).collect() }
}

fn info(runtime: Runtime) -> RuntimeInfo {
    let win = cfg!(windows);
    let mac = cfg!(target_os = "macos");
    let unix = |windows: &'static str, other: &'static str| if win { windows } else { other };
    let (label, blurb, source, unavailable): (&str, &str, &str, Option<&str>) = match runtime {
        Runtime::Apache => ("Apache", "Serves your sites", unix("Apache Lounge builds", "LampRabit's builds of Apache httpd"), None),
        Runtime::Nginx => ("Nginx", "Serves your sites", unix("Official nginx.org Windows builds (PGP-signed, no checksum)", "LampRabit's builds of nginx.org releases"), None),
        Runtime::Php => ("PHP", "Runs your code", unix("Thread-safe builds from windows.php.net", "LampRabit's static builds with common extensions built in, Redis and Memcached clients included"), None),
        Runtime::Caddy => ("Caddy", "Serves your sites", "Official Caddy releases from GitHub; uses LampRabit's certificates and PHP, not its own", None),
        Runtime::Frankenphp => (
            "FrankenPHP",
            "Serves your sites",
            "Official FrankenPHP releases: Caddy with its own PHP built in, so LampRabit's PHP versions and extensions don't apply to its sites",
            (mac && cfg!(target_arch = "x86_64")).then_some("FrankenPHP publishes no build for Intel Macs."),
        ),
        Runtime::Tomcat => ("Tomcat", "Runs Java web apps", "Official Apache Tomcat releases (SHA-512 checked); runs on the active Java and serves Java projects behind your web server", None),
        Runtime::Java => ("Java", "Runs Java apps", "Eclipse Temurin JDKs from Adoptium (checksum checked); java, javac and jar work in LampRabit's terminal, with JAVA_HOME set", None),
        Runtime::Mariadb => (
            "MariaDB",
            "Stores your data",
            "Official MariaDB releases",
            mac.then_some("MariaDB publishes no macOS builds, and LampRabit doesn't build its own yet. Use PostgreSQL, or MariaDB from Homebrew on its own port."),
        ),
        Runtime::Mysql => (
            "MySQL",
            "Stores your data",
            "Official MySQL releases; runs next to MariaDB on its own port",
            mac.then_some("MySQL on macOS isn't supported by LampRabit yet. Use PostgreSQL, or MySQL from Homebrew on its own port."),
        ),
        Runtime::Postgres => ("PostgreSQL", "Stores your data", "PostgreSQL built from source for every OS by theseus-rs (checksum checked); psql and pg_dump work in LampRabit's terminal", None),
        Runtime::Mongodb => ("MongoDB", "Stores your documents", "Official MongoDB Community Server releases (checksum checked); each major version keeps its own data folder", None),
        Runtime::Mongosh => ("mongosh", "MongoDB shell", "MongoDB's shell from its GitHub releases; mongosh works in LampRabit's terminal, and MongoDB uses it to shut down cleanly", None),
        Runtime::Mongotools => ("MongoDB Database Tools", "MongoDB backups", "MongoDB's mongodump and mongorestore (checksum checked); LampRabit's MongoDB backups use them", None),
        Runtime::Compass => ("MongoDB Compass", "MongoDB GUI", "MongoDB's desktop app from its GitHub releases; opens connected to LampRabit's MongoDB", None),
        Runtime::Sqlite => ("SQLite", "File databases", "The sqlite3 shell from sqlite.org (SHA3 checked). PHP has SQLite built in, so projects don't need this to use it; backups use it to copy safely", None),
        Runtime::Clickhouse => (
            "ClickHouse",
            "Analytics database",
            "Official ClickHouse builds from GitHub. Its web console is at /play on its HTTP port",
            win.then_some("ClickHouse has no Windows build. Run it in WSL, or use LampRabit on Linux or macOS."),
        ),
        Runtime::Neo4j => ("Neo4j", "Graph database", "Neo4j Community from dist.neo4j.org (checksum checked); runs on Java 21+, Neo4j Browser at neo4j.test", None),
        Runtime::Meilisearch => ("Meilisearch", "Search engine", "Official Meilisearch Community builds from GitHub (checksum checked); runs in development mode, with its search preview at its URL", None),
        Runtime::Typesense => (
            "Typesense",
            "Search engine",
            "Official Typesense builds from dl.typesense.org (no checksums published). API key: lamprabit",
            win.then_some("Typesense has no Windows build. Run it in WSL, or use Meilisearch."),
        ),
        Runtime::Elasticsearch => ("Elasticsearch", "Search engine", "Official Elasticsearch archives (SHA-512 checked) with their own Java; one node, security off, 512 MB of memory", None),
        Runtime::Opensearch => (
            "OpenSearch",
            "Search engine",
            "Official OpenSearch archives (SHA-512 checked) with their own Java; one node, security off, 512 MB of memory",
            mac.then_some("OpenSearch has no macOS build. Use Elasticsearch, which works the same for most apps."),
        ),
        Runtime::Rustfs => ("RustFS", "S3 object storage", "RustFS from its GitHub releases (checksum checked): S3-compatible, with a web console. Access key lamprabit, secret lamprabit-secret", None),
        Runtime::Minio => (
            "MinIO",
            "S3 object storage",
            "MinIO's last release (October 2025), built from its source by LampRabit's CI: the project is archived, so there are no security fixes. Access key lamprabit, secret lamprabit-secret",
            None,
        ),
        Runtime::N8n => ("n8n", "Workflow automation", "n8n from npm, installed into its own folder and run on NVM's Node.js (24 or newer). Editor at n8n.test", None),
        Runtime::Rabbitmq => ("RabbitMQ", "Message queue", "Official RabbitMQ releases (checksum checked) with the management console (user guest, password guest)", None),
        Runtime::Erlang => (
            "Erlang",
            "Runs RabbitMQ",
            "Official Erlang/OTP builds for Windows (checksum checked)",
            (!win).then_some("Erlang has no official Linux or macOS builds: install it with your system's packages (sudo apt install erlang-nox, sudo dnf install erlang, or brew install erlang); RabbitMQ uses it."),
        ),
        Runtime::Ollama => ("Ollama", "Local AI models", "Ollama's own builds from GitHub (a large download). Models go to LampRabit's data folder; the ollama command uses this server", None),
        Runtime::Go => ("Go", "Runs Go code", "Official Go releases from go.dev (checksum checked); go and gofmt work in LampRabit's terminal", None),
        Runtime::Rust => (
            "Rust",
            "Runs Rust code",
            unix(
                "Rust stable through rustup, kept inside LampRabit's folder (not your ~/.cargo), with the GNU toolchain, which needs no Visual Studio",
                "Rust stable through rustup, kept inside LampRabit's folder (not your ~/.cargo)",
            ),
            None,
        ),
        Runtime::Python => ("Python", "Runs Python code", "python-build-standalone (the builds uv uses, checksum checked); python and pip work in LampRabit's terminal", None),
        Runtime::Phppgadmin => ("phpPgAdmin", "PostgreSQL admin", "The PostgreSQL web interface in PHP (ReimuHakurei's maintained fork for PHP 8); log in as root without a password", None),
        Runtime::Pgadmin => ("pgAdmin", "PostgreSQL admin", "PostgreSQL's official admin tool from PyPI, in its own Python environment (needs Python 3.10–3.14); LampRabit's PostgreSQL is added for you", None),
        Runtime::Mailpit => ("Mailpit", "Catches outgoing mail", "Local SMTP server with a web inbox", None),
        Runtime::Adminer => ("Adminer", "Database admin", "A single-file database manager", None),
        Runtime::Phpmyadmin => ("phpMyAdmin", "Database admin", "The classic MySQL/MariaDB web interface", None),
        Runtime::Ngrok => ("Ngrok", "Public tunnels", "Share sites on a public URL (free account needed)", None),
        Runtime::Cloudflared => ("Cloudflare Tunnel", "Public tunnels", "Cloudflare quick tunnels, no account needed", None),
        Runtime::Redis => (
            "Redis",
            "Cache and queues",
            unix("Real Redis from the community redis-windows build (Redis has no official Windows build), with redis-cli", "LampRabit's builds of Redis, with redis-cli"),
            None,
        ),
        Runtime::Composer => ("Composer", "PHP packages", "PHP's package manager; run composer from the LampRabit terminal", None),
        Runtime::Maven => ("Maven", "Java builds", "Apache Maven from Apache's mirror (SHA-512 checked); mvn works in LampRabit's terminal on the active Java", None),
        Runtime::Gradle => ("Gradle", "Java builds", "Gradle's official distribution from services.gradle.org (SHA-256 checked); gradle works in LampRabit's terminal on the active Java", None),
        Runtime::Phpredisadmin => ("phpRedisAdmin", "Redis admin", "Browse and edit Redis keys in the browser, like phpMyAdmin", None),
        Runtime::Memcached => (
            "Memcached",
            "Memory cache",
            unix("nono303's Windows build of Memcached (no official one exists), checked and repackaged by LampRabit", "LampRabit's builds of Memcached"),
            None,
        ),
    };
    let suggests: &[&str] = match runtime {
        Runtime::Php => &["composer"],
        Runtime::Mariadb | Runtime::Mysql => &["phpmyadmin", "adminer"],
        Runtime::Postgres => &["adminer", "phppgadmin", "pgadmin"],
        Runtime::Mongodb => &["mongosh", "compass", "mongotools"],
        Runtime::Redis => &["phpredisadmin"],
        _ => &[],
    };
    RuntimeInfo {
        id: runtime.id(),
        label,
        blurb,
        source,
        unavailable,
        needs: needs(runtime, None).into_iter().map(|n| n.runtime).collect(),
        suggests: suggests.to_vec(),
    }
}

/// One thing a runtime needs, and which of its versions work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Need {
    /// A runtime id, or "node" for NVM's Node.js.
    pub runtime: &'static str,
    /// Oldest version that works; the comparison stops at its last part ("17" takes 17.x).
    pub min: Option<String>,
    /// Newest version that works, by prefix ("3.14" takes 3.14.x).
    pub max: Option<String>,
    /// The version an install picks when several fit (pgAdmin's wheels are built for 3.13).
    pub prefer: Option<&'static str>,
    pub why: &'static str,
}

fn need(runtime: &'static str, min: Option<&str>, max: Option<&str>, why: &'static str) -> Need {
    Need { runtime, min: min.map(str::to_string), max: max.map(str::to_string), prefer: None, why }
}

/// What a runtime needs at `version` (or at any version, for listing). Kept in step with the
/// checks that refuse to start without it (`configs::tomcat_java`, `configs::neo4j_java`,
/// `runtimes::n8n_node`), so whatever this installs is accepted there.
pub fn needs(runtime: Runtime, version: Option<&str>) -> Vec<Need> {
    let major = version.and_then(|v| parts(v).first().copied());
    match runtime {
        // Composer 2.2 (its LTS line) still runs on PHP 5.3; 2.3 and later need 7.2.5.
        Runtime::Composer => {
            let php = if version.is_some_and(|v| cmp(&parts(v), &[2, 3]).is_lt()) { "5.3.2" } else { "7.2.5" };
            vec![need("php", Some(php), None, "Composer runs on PHP")]
        }
        Runtime::Tomcat => {
            let java = match major {
                Some(11..) | None => "17",
                Some(10) => "11",
                Some(_) => "8",
            };
            vec![need("java", Some(java), None, "Tomcat runs on the active Java")]
        }
        Runtime::Neo4j => vec![need("java", Some("21"), None, "Neo4j runs on the active Java")],
        // Maven 4 needs Java 17; Maven 3 runs on 8 and anything newer.
        Runtime::Maven => vec![need("java", Some(if major.is_none_or(|m| m >= 4) { "17" } else { "8" }), None, "Maven runs on the active Java")],
        Runtime::Gradle => {
            let (min, max) = version.map_or(("17", Some("25")), gradle_java);
            vec![need("java", Some(min), max, "Gradle runs on the active Java, and each version only up to a newest Java")]
        }
        Runtime::N8n => vec![need("node", Some("24"), None, "n8n runs on NVM's default Node.js")],
        Runtime::Pgadmin => vec![
            Need { prefer: Some("3.13"), ..need("python", Some("3.10"), Some("3.14"), "pgAdmin runs in a Python environment") },
            need("postgres", None, None, "pgAdmin manages LampRabit's PostgreSQL"),
        ],
        Runtime::Phppgadmin => vec![need("postgres", None, None, "phpPgAdmin manages LampRabit's PostgreSQL"), need("php", Some("8.0"), None, "phpPgAdmin runs on PHP")],
        Runtime::Phpredisadmin => vec![need("redis", None, None, "phpRedisAdmin browses LampRabit's Redis"), need("php", Some("7.2"), None, "phpRedisAdmin runs on PHP")],
        Runtime::Phpmyadmin | Runtime::Adminer => vec![need("php", None, None, "It runs on PHP")],
        Runtime::Mongosh | Runtime::Mongotools | Runtime::Compass => vec![need("mongodb", None, None, "It works with LampRabit's MongoDB")],
        // Elsewhere RabbitMQ runs on the system's Erlang, which LampRabit can't install.
        Runtime::Rabbitmq if cfg!(windows) => {
            let (min, max) = version.map_or(("27.0", "29"), rabbitmq_erlang);
            vec![need("erlang", Some(min), Some(max), "RabbitMQ runs on Erlang")]
        }
        _ => vec![],
    }
}

/// The Erlang/OTP versions each RabbitMQ release supports, from rabbitmq.com/docs/which-erlang
/// (checked 2026-11-04). Newer releases than the table knows get its newest row.
fn rabbitmq_erlang(version: &str) -> (&'static str, &'static str) {
    let v = parts(version);
    let at_least = |m: &[u64]| cmp(&v, m) != std::cmp::Ordering::Less;
    if at_least(&[4, 4]) || at_least(&[4, 3, 7]) {
        ("27.0", "29")
    } else if at_least(&[4, 3, 6]) {
        ("27.0", "28")
    } else if at_least(&[4, 3, 3]) || (at_least(&[4, 2, 9]) && !at_least(&[4, 3])) {
        ("27.0", "27")
    } else if at_least(&[4, 0, 4]) {
        ("26.2", "27")
    } else {
        ("26.2", "26.2")
    }
}

/// The Java versions each Gradle release can run on, from Gradle's compatibility matrix
/// (docs.gradle.org/current/userguide/compatibility.html, checked 2026-11-04). Releases newer
/// than it knows get the newest row without an upper bound.
fn gradle_java(version: &str) -> (&'static str, Option<&'static str>) {
    let v = parts(version);
    let at_least = |m: &[u64]| cmp(&v, m) != std::cmp::Ordering::Less;
    if at_least(&[10]) {
        ("17", None)
    } else if at_least(&[9, 8]) {
        ("17", Some("27"))
    } else if at_least(&[9, 4]) {
        ("17", Some("26"))
    } else if at_least(&[9, 1]) {
        ("17", Some("25"))
    } else if at_least(&[9]) {
        ("17", Some("24"))
    } else if at_least(&[8, 14]) {
        ("8", Some("24"))
    } else if at_least(&[8, 10]) {
        ("8", Some("23"))
    } else if at_least(&[8, 8]) {
        ("8", Some("22"))
    } else if at_least(&[8, 5]) {
        ("8", Some("21"))
    } else if at_least(&[8, 3]) {
        ("8", Some("20"))
    } else {
        ("8", Some("19"))
    }
}

/// The numeric parts of a version: "v21.0.5+11" → [21, 0, 5].
fn parts(version: &str) -> Vec<u64> {
    version
        .trim_start_matches(['v', 'V'])
        .split(|c: char| c == '.' || c == '+' || c == '-' || c == '_')
        .map_while(|p| p.parse().ok())
        .collect()
}

/// Compares only as many parts as `bound` has, missing parts counting as 0.
fn cmp(version: &[u64], bound: &[u64]) -> std::cmp::Ordering {
    (0..bound.len()).map(|i| version.get(i).unwrap_or(&0).cmp(&bound[i])).find(|o| o.is_ne()).unwrap_or(std::cmp::Ordering::Equal)
}

impl Need {
    pub fn fits(&self, version: &str) -> bool {
        let v = parts(version);
        self.min.as_deref().is_none_or(|m| cmp(&v, &parts(m)).is_ge()) && self.max.as_deref().is_none_or(|m| cmp(&v, &parts(m)).is_le())
    }

    /// "17 or newer", "3.10 to 3.14", "26.2 to 27.x", or "" for any version.
    fn range(&self) -> String {
        match (&self.min, &self.max) {
            (Some(min), Some(max)) if min == max => format!("{max}.x"),
            (Some(min), Some(max)) => format!("{min} to {max}.x"),
            (Some(min), None) => format!("{min} or newer"),
            (None, Some(max)) => format!("{max}.x or older"),
            (None, None) => String::new(),
        }
    }
}

/// What happens to one need before the runtime is installed.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Action {
    /// The active version fits already.
    Ready { version: String },
    /// An installed version fits but isn't the active one: it becomes active.
    Activate { version: String },
    /// Downloaded, then made active.
    Install { release: Release },
    /// A Node.js version installed through NVM, then made the default.
    InstallNode { version: String },
    /// NVM's Node.js that fits becomes the default.
    UseNode { version: String },
    /// Can't be done here; the reason says what to do instead.
    Unavailable { reason: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Requirement {
    pub runtime: &'static str,
    pub label: String,
    pub why: &'static str,
    /// Empty when any version works.
    pub range: String,
    pub action: Action,
}

/// What `runtime` at `version` (its active version, else its newest download, when None)
/// needs, and how each need gets met: the active version, another installed one, or the
/// download that fits.
pub async fn requirements(app: &AppHandle, client: &reqwest::Client, settings: &Settings, layout: &Layout, runtime: Runtime, version: Option<String>) -> Result<Vec<Requirement>> {
    let version = match version.or_else(|| settings.versions.get(runtime.id()).cloned()) {
        Some(v) => Some(v),
        None if needs(runtime, None).iter().any(|n| n.min.is_some() || n.max.is_some()) => {
            runtimes::available(client, runtime, false).await.ok().and_then(|c| c.first().map(|r| r.version.clone()))
        }
        None => None,
    };
    let mut out = Vec::new();
    for need in needs(runtime, version.as_deref()) {
        let label = Runtime::from_id(need.runtime).map_or("Node.js", |r| info(r).label).to_string();
        let action = if need.runtime == "node" { node_action(app, client, &need).await } else { runtime_action(client, settings, layout, &need, &label).await };
        out.push(Requirement { runtime: need.runtime, label, why: need.why, range: need.range(), action });
    }
    Ok(out)
}

async fn runtime_action(client: &reqwest::Client, settings: &Settings, layout: &Layout, need: &Need, label: &str) -> Action {
    let Some(runtime) = Runtime::from_id(need.runtime) else {
        return Action::Unavailable { reason: format!("{label} can't be installed here") };
    };
    if let Some(active) = settings.versions.get(need.runtime).filter(|v| need.fits(v)) {
        return Action::Ready { version: active.clone() };
    }
    let installed = runtimes::installed(layout, runtime);
    if let Some(version) = newest(installed.iter().map(String::as_str), need) {
        return Action::Activate { version: version.to_string() };
    }
    if let Some(reason) = info(runtime).unavailable {
        return Action::Unavailable { reason: reason.to_string() };
    }
    match runtimes::available(client, runtime, false).await {
        Ok(catalog) => {
            let fitting = || catalog.iter().filter(|r| need.fits(&r.version));
            // Catalogs are newest first; an LTS line is the safer pick when there is one.
            let pick = need
                .prefer
                .and_then(|p| fitting().find(|r| r.version.starts_with(&format!("{p}."))))
                .or_else(|| fitting().find(|r| r.label.as_deref() == Some("LTS")))
                .or_else(|| fitting().next());
            match pick {
                Some(release) => Action::Install { release: release.clone() },
                None => Action::Unavailable { reason: format!("No {label} {} download for this platform", need.range()).split_whitespace().collect::<Vec<_>>().join(" ") },
            }
        }
        Err(e) => Action::Unavailable { reason: e.to_string() },
    }
}

async fn node_action(app: &AppHandle, client: &reqwest::Client, need: &Need) -> Action {
    let status = crate::node::status(app).await;
    if status.manager.is_none() {
        return Action::Unavailable { reason: "Node.js comes through NVM: install NVM on the Services page (Languages → Node.js) first.".into() };
    }
    if let Some(current) = status.current.as_deref().filter(|v| need.fits(v)) {
        return Action::Ready { version: current.to_string() };
    }
    if let Some(version) = newest(status.installed.iter().map(String::as_str), need) {
        return Action::UseNode { version: version.to_string() };
    }
    match crate::node::available(client).await {
        Ok(list) => {
            let fits = |r: &&crate::node::NodeRelease| need.fits(&r.version);
            match list.iter().filter(fits).find(|r| r.lts.is_some()).or_else(|| list.iter().find(fits)) {
                Some(r) => Action::InstallNode { version: r.version.trim_start_matches('v').to_string() },
                None => Action::Unavailable { reason: format!("No Node.js {} release found", need.range()) },
            }
        }
        Err(e) => Action::Unavailable { reason: e.to_string() },
    }
}

fn newest<'a>(versions: impl Iterator<Item = &'a str>, need: &Need) -> Option<&'a str> {
    versions.filter(|v| need.fits(v)).max_by(|a, b| cmp(&parts(a), &parts(b)).then(parts(a).len().cmp(&parts(b).len())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_compare_by_the_bounds_parts() {
        let java17 = need("java", Some("17"), None, "");
        assert!(java17.fits("17.0.13+11") && java17.fits("21.0.5") && !java17.fits("11.0.25"));
        let python = need("python", Some("3.10"), Some("3.14"), "");
        assert!(python.fits("3.14.2") && python.fits("3.10.0") && !python.fits("3.9.21") && !python.fits("3.15.0"));
        assert!(need("node", Some("24"), None, "").fits("v24.11.1"));
        assert_eq!(python.range(), "3.10 to 3.14.x");
        assert_eq!(java17.range(), "17 or newer");
    }

    #[test]
    fn tomcat_lines_need_their_java() {
        let java = |v| needs(Runtime::Tomcat, Some(v))[0].min.clone().unwrap();
        assert_eq!(java("9.0.112"), "8");
        assert_eq!(java("10.1.48"), "11");
        assert_eq!(java("11.0.14"), "17");
    }

    #[test]
    fn rabbitmq_follows_its_erlang_table() {
        assert_eq!(rabbitmq_erlang("4.0.2"), ("26.2", "26.2"));
        assert_eq!(rabbitmq_erlang("4.1.4"), ("26.2", "27"));
        assert_eq!(rabbitmq_erlang("4.2.8"), ("26.2", "27"));
        assert_eq!(rabbitmq_erlang("4.2.10"), ("27.0", "27"));
        assert_eq!(rabbitmq_erlang("4.3.6"), ("27.0", "28"));
        assert_eq!(rabbitmq_erlang("4.4.0"), ("27.0", "29"));
        let otp = need("erlang", Some("26.2"), Some("27"), "");
        assert!(otp.fits("27.3.4") && otp.fits("26.2.5") && !otp.fits("28.1") && !otp.fits("26.1"));
    }

    #[test]
    fn build_tools_need_a_java_they_run_on() {
        let java = |r, v| needs(r, Some(v)).remove(0);
        assert_eq!(java(Runtime::Maven, "3.9.11").min.as_deref(), Some("8"));
        assert_eq!(java(Runtime::Maven, "4.0.1").min.as_deref(), Some("17"));
        let gradle9 = java(Runtime::Gradle, "9.2.1");
        assert!(gradle9.fits("21.0.9") && gradle9.fits("25.0.1") && !gradle9.fits("11.0.29"));
        let gradle8 = java(Runtime::Gradle, "8.14.3");
        assert!(gradle8.fits("8.0.472") && gradle8.fits("21.0.9") && !gradle8.fits("25.0.1"), "Gradle 8 can't run on Java 25");
        assert!(java(Runtime::Gradle, "9.8.0").fits("27.0.1") && !java(Runtime::Gradle, "9.4.1").fits("27.0.1"));
    }

    #[test]
    fn every_runtime_is_grouped_and_named() {
        let catalog = catalog();
        assert!(catalog.groups.iter().all(|g| g.id != "other"), "add new runtimes to a group");
        for r in &catalog.runtimes {
            assert!(!r.label.is_empty() && !r.source.is_empty(), "{}", r.id);
            for id in r.needs.iter().chain(&r.suggests) {
                assert!(*id == "node" || Runtime::from_id(id).is_some(), "{} names {id}", r.id);
            }
        }
    }
}
