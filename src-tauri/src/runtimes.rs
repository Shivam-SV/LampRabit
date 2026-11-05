//! Runtime catalog (what can be downloaded) and installer (download, verify, extract).

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

use crate::{
    error::{Error, Result},
    layout::{Layout, exe},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    Apache,
    Nginx,
    Php,
    Mariadb,
    /// Developer tools: a mail catcher and two database web UIs.
    Mailpit,
    Adminer,
    Phpmyadmin,
    /// Public tunnels for sharing sites.
    Ngrok,
    Cloudflared,
    /// Redis with redis-cli: LampRabit's builds on Linux and macOS, the community
    /// redis-windows build (Redis compiled with MSYS2) on Windows.
    Redis,
    /// PHP's package manager, a single `composer.phar` run by the active PHP.
    Composer,
    /// Oracle MySQL, next to MariaDB on its own port.
    Mysql,
    /// Web interface for Redis, served like phpMyAdmin.
    Phpredisadmin,
    /// Memcached: LampRabit's builds on Linux and macOS, nono303's Cygwin build
    /// (repackaged by LampRabit's CI) on Windows.
    Memcached,
    /// Caddy, a single binary from its GitHub releases.
    Caddy,
    /// Java (Eclipse Temurin JDKs from Adoptium), for Java projects and the services that
    /// run on it (Tomcat, Neo4j, OpenSearch).
    Java,
    /// Apache Tomcat, the servlet container for Java web apps; runs on the active Java.
    Tomcat,
    /// FrankenPHP: Caddy with PHP built in, from its GitHub releases.
    Frankenphp,
    /// PostgreSQL, from theseus-rs/postgresql-binaries (built from source for every OS).
    Postgres,
    /// Python, from astral-sh/python-build-standalone (relocatable builds, what uv uses).
    Python,
    /// Web interface for PostgreSQL, served like phpMyAdmin (a maintained fork that runs
    /// on PHP 8; the original stopped at PHP 7).
    Phppgadmin,
    /// pgAdmin 4, PostgreSQL's own admin tool: a Python web app in its own virtual
    /// environment on the active Python.
    Pgadmin,
    /// MongoDB Community Server, from MongoDB's own downloads.
    Mongodb,
    /// MongoDB's shell, from its GitHub releases.
    Mongosh,
    /// MongoDB Database Tools (mongodump, mongorestore…), for backups.
    Mongotools,
    /// MongoDB Compass, MongoDB's desktop app for browsing data.
    Compass,
    /// The sqlite3 command-line shell from sqlite.org (PHP has SQLite built in).
    Sqlite,
    /// ClickHouse, a single binary (Linux and macOS only).
    Clickhouse,
    /// Neo4j Community, a graph database on the active Java.
    Neo4j,
    /// Meilisearch, a search engine (single binary).
    Meilisearch,
    /// Typesense, a search engine (Linux and macOS only).
    Typesense,
    /// RustFS, S3-compatible object storage.
    Rustfs,
    /// Ollama, for running language models locally.
    Ollama,
    /// MinIO's last release, built from source by LampRabit's CI (the project is archived).
    Minio,
    /// Elasticsearch, with its bundled Java.
    Elasticsearch,
    /// OpenSearch, with its bundled Java (no macOS build).
    Opensearch,
    /// n8n, an npm package installed into its own folder and run on NVM's Node.
    N8n,
    /// RabbitMQ, on Erlang.
    Rabbitmq,
    /// Erlang/OTP for RabbitMQ: official builds on Windows only.
    Erlang,
    /// Go, from go.dev.
    Go,
    /// Apache Maven, Java's build tool, on the active Java.
    Maven,
    /// Gradle, Java's other build tool, on the active Java.
    Gradle,
    /// Rust through rustup, with its homes inside LampRabit's folder.
    Rust,
}

impl Runtime {
    pub const ALL: [Runtime; 43] = [
        Runtime::Apache,
        Runtime::Nginx,
        Runtime::Php,
        Runtime::Mariadb,
        Runtime::Mailpit,
        Runtime::Adminer,
        Runtime::Phpmyadmin,
        Runtime::Ngrok,
        Runtime::Cloudflared,
        Runtime::Redis,
        Runtime::Composer,
        Runtime::Mysql,
        Runtime::Phpredisadmin,
        Runtime::Memcached,
        Runtime::Caddy,
        Runtime::Java,
        Runtime::Tomcat,
        Runtime::Frankenphp,
        Runtime::Postgres,
        Runtime::Python,
        Runtime::Phppgadmin,
        Runtime::Pgadmin,
        Runtime::Mongodb,
        Runtime::Mongosh,
        Runtime::Mongotools,
        Runtime::Compass,
        Runtime::Sqlite,
        Runtime::Clickhouse,
        Runtime::Neo4j,
        Runtime::Meilisearch,
        Runtime::Typesense,
        Runtime::Rustfs,
        Runtime::Ollama,
        Runtime::Minio,
        Runtime::Elasticsearch,
        Runtime::Opensearch,
        Runtime::N8n,
        Runtime::Rabbitmq,
        Runtime::Erlang,
        Runtime::Go,
        Runtime::Rust,
        Runtime::Maven,
        Runtime::Gradle,
    ];

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.id() == id)
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Apache => "apache",
            Self::Nginx => "nginx",
            Self::Php => "php",
            Self::Mariadb => "mariadb",
            Self::Mailpit => "mailpit",
            Self::Adminer => "adminer",
            Self::Phpmyadmin => "phpmyadmin",
            Self::Ngrok => "ngrok",
            Self::Cloudflared => "cloudflared",
            Self::Redis => "redis",
            Self::Composer => "composer",
            Self::Mysql => "mysql",
            Self::Phpredisadmin => "phpredisadmin",
            Self::Memcached => "memcached",
            Self::Caddy => "caddy",
            Self::Java => "java",
            Self::Tomcat => "tomcat",
            Self::Frankenphp => "frankenphp",
            Self::Postgres => "postgres",
            Self::Python => "python",
            Self::Phppgadmin => "phppgadmin",
            Self::Pgadmin => "pgadmin",
            Self::Mongodb => "mongodb",
            Self::Mongosh => "mongosh",
            Self::Mongotools => "mongotools",
            Self::Compass => "compass",
            Self::Sqlite => "sqlite",
            Self::Clickhouse => "clickhouse",
            Self::Neo4j => "neo4j",
            Self::Meilisearch => "meilisearch",
            Self::Typesense => "typesense",
            Self::Rustfs => "rustfs",
            Self::Ollama => "ollama",
            Self::Minio => "minio",
            Self::Elasticsearch => "elasticsearch",
            Self::Opensearch => "opensearch",
            Self::N8n => "n8n",
            Self::Rabbitmq => "rabbitmq",
            Self::Erlang => "erlang",
            Self::Go => "go",
            Self::Rust => "rust",
            Self::Maven => "maven",
            Self::Gradle => "gradle",
        }
    }

    /// File that must exist inside an installed version's directory.
    pub fn marker(self) -> PathBuf {
        match self {
            Self::Apache => Path::new("bin").join(exe("httpd")),
            Self::Nginx => PathBuf::from(exe("nginx")),
            Self::Php if cfg!(windows) => PathBuf::from(exe("php")),
            Self::Php => PathBuf::from("php-fpm"),
            Self::Mariadb => Path::new("bin").join(exe("mariadbd")),
            Self::Mailpit => PathBuf::from(exe("mailpit")),
            Self::Adminer => PathBuf::from("adminer.php"),
            Self::Phpmyadmin => PathBuf::from("index.php"),
            Self::Ngrok => PathBuf::from(exe("ngrok")),
            Self::Cloudflared => PathBuf::from(exe("cloudflared")),
            Self::Redis => PathBuf::from(exe("redis-server")),
            Self::Composer => PathBuf::from("composer.phar"),
            Self::Mysql => Path::new("bin").join(exe("mysqld")),
            Self::Phpredisadmin => PathBuf::from("index.php"),
            Self::Memcached => PathBuf::from(exe("memcached")),
            Self::Caddy => PathBuf::from(exe("caddy")),
            Self::Java => Path::new("bin").join(exe("java")),
            Self::Tomcat => Path::new("bin").join("bootstrap.jar"),
            Self::Frankenphp => PathBuf::from(exe("frankenphp")),
            Self::Postgres => Path::new("bin").join(exe("postgres")),
            Self::Python if cfg!(windows) => PathBuf::from("python.exe"),
            Self::Python => Path::new("bin").join("python3"),
            Self::Phppgadmin => PathBuf::from("index.php"),
            // Written last by `setup_pgadmin`, so a half-finished install doesn't count.
            Self::Pgadmin => PathBuf::from("installed.txt"),
            Self::Mongodb => Path::new("bin").join(exe("mongod")),
            Self::Mongosh => Path::new("bin").join(exe("mongosh")),
            Self::Mongotools => Path::new("bin").join(exe("mongodump")),
            // Compass is an Electron app: its executable's name varies, its app bundle doesn't.
            Self::Compass if cfg!(target_os = "macos") => Path::new("MongoDB Compass.app").join("Contents").join("Resources").join("app.asar"),
            Self::Compass => Path::new("resources").join("app.asar"),
            Self::Sqlite => PathBuf::from(exe("sqlite3")),
            Self::Clickhouse => Path::new("usr").join("bin").join("clickhouse"),
            Self::Neo4j => Path::new("bin").join(if cfg!(windows) { "neo4j.bat" } else { "neo4j" }),
            Self::Meilisearch => PathBuf::from(exe("meilisearch")),
            Self::Typesense => PathBuf::from("typesense-server"),
            Self::Rustfs => PathBuf::from(exe("rustfs")),
            // Linux archives put it in bin/ (next to lib/ollama); Windows and macOS at the top.
            Self::Ollama if cfg!(target_os = "linux") => Path::new("bin").join("ollama"),
            Self::Ollama => PathBuf::from(exe("ollama")),
            Self::Minio => PathBuf::from(exe("minio")),
            Self::Elasticsearch => Path::new("bin").join(if cfg!(windows) { "elasticsearch.bat" } else { "elasticsearch" }),
            Self::Opensearch => Path::new("bin").join(if cfg!(windows) { "opensearch.bat" } else { "opensearch" }),
            // Written last by `setup_n8n`, so a half-finished install doesn't count.
            Self::N8n => PathBuf::from("installed.txt"),
            Self::Rabbitmq => Path::new("sbin").join(if cfg!(windows) { "rabbitmq-server.bat" } else { "rabbitmq-server" }),
            Self::Erlang => Path::new("bin").join(exe("erl")),
            Self::Go => Path::new("bin").join(exe("go")),
            // Written last by `setup_rust`, so a half-finished install doesn't count.
            Self::Rust => PathBuf::from("installed.txt"),
            Self::Maven => Path::new("bin").join("mvn"),
            Self::Gradle => Path::new("bin").join("gradle"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Archive {
    Zip,
    TarGz,
    TarXz,
    /// Zstandard-compressed tar (Ollama on Linux).
    TarZst,
    /// A single file, saved as the runtime's marker file.
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub runtime: Runtime,
    pub version: String,
    pub url: String,
    pub sha256: Option<String>,
    /// For publishers that only give SHA-512.
    #[serde(default)]
    pub sha512: Option<String>,
    /// MySQL only publishes MD5: it catches corrupt downloads, HTTPS covers the rest.
    #[serde(default)]
    pub md5: Option<String>,
    /// SQLite publishes SHA3-256.
    #[serde(default)]
    pub sha3_256: Option<String>,
    pub archive: Archive,
    /// Short note shown next to the version, e.g. "LTS".
    pub label: Option<String>,
}

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("lampRabit/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .expect("http client")
}

fn unsupported(what: &str) -> Error {
    Error::msg(format!("{what} downloads are not available for this OS yet"))
}

fn http(e: reqwest::Error) -> Error {
    Error::msg(format!("network error: {e}"))
}

/// A version's release line: "8.4" for "8.4.27".
fn line(v: &str) -> String {
    v.split('.').take(2).collect::<Vec<_>>().join(".")
}

fn version_key(v: &str) -> Vec<u32> {
    v.split(|c: char| !c.is_ascii_digit()).filter_map(|p| p.parse().ok()).collect()
}

/// How long a downloaded catalog is used before it's fetched again.
const CATALOG_TTL: Duration = Duration::from_secs(6 * 60 * 60);

fn catalog_file(runtime: Runtime) -> Option<PathBuf> {
    Some(dirs::cache_dir()?.join("LampRabit").join("catalogs").join(format!("{}-{}.json", runtime.id(), std::env::consts::OS)))
}

/// The versions available for a runtime. Catalogs are kept on disk for a few hours: most
/// come from GitHub's API, which allows few anonymous requests an hour, and browsing the
/// Services page would otherwise use them up. `fresh` (the refresh button) fetches anew; a
/// failed fetch falls back to the last catalog, however old.
pub async fn available(client: &reqwest::Client, runtime: Runtime, fresh: bool) -> Result<Vec<Release>> {
    let file = catalog_file(runtime);
    let cached = file.as_ref().and_then(|f| {
        let age = std::fs::metadata(f).ok()?.modified().ok()?.elapsed().unwrap_or_default();
        let releases: Vec<Release> = serde_json::from_slice(&std::fs::read(f).ok()?).ok()?;
        Some((age, releases))
    });
    if let Some((age, releases)) = &cached {
        if !fresh && *age < CATALOG_TTL && !releases.is_empty() {
            return Ok(releases.clone());
        }
    }
    match fetch_available(client, runtime).await {
        Ok(releases) => {
            if let (Some(f), Ok(json)) = (&file, serde_json::to_vec(&releases)) {
                let _ = std::fs::create_dir_all(f.parent().unwrap_or(f));
                let _ = std::fs::write(f, json);
            }
            Ok(releases)
        }
        Err(e) => match cached {
            Some((_, releases)) if !releases.is_empty() => Ok(releases),
            _ if e.to_string().contains("rate limit") => Err(Error::msg(
                "GitHub's limit for downloads lists was reached (60 an hour without an account). Try again within the hour, or set a GITHUB_TOKEN environment variable and restart LampRabit.",
            )),
            _ => Err(e),
        },
    }
}

async fn fetch_available(client: &reqwest::Client, runtime: Runtime) -> Result<Vec<Release>> {
    let mut releases = match runtime {
        Runtime::Php => php_releases(client).await?,
        Runtime::Apache => apache_releases(client).await?,
        Runtime::Nginx => nginx_releases(client).await?,
        Runtime::Mariadb => mariadb_releases(client).await?,
        Runtime::Mailpit => mailpit_releases(client).await?,
        Runtime::Adminer => adminer_releases(client).await?,
        Runtime::Phpmyadmin => phpmyadmin_releases(client).await?,
        Runtime::Ngrok => ngrok_releases(),
        Runtime::Cloudflared => cloudflared_releases(client).await?,
        Runtime::Redis if cfg!(windows) => redis_windows_releases(client).await?,
        Runtime::Redis => own_builds(client, Runtime::Redis).await?,
        Runtime::Composer => composer_releases(client).await?,
        Runtime::Mysql => mysql_releases(client).await?,
        Runtime::Phpredisadmin => phpredisadmin_releases(client).await?,
        Runtime::Memcached => own_builds(client, Runtime::Memcached).await?,
        Runtime::Caddy => caddy_releases(client).await?,
        Runtime::Java => java_releases(client).await?,
        Runtime::Tomcat => tomcat_releases(client).await?,
        Runtime::Frankenphp => frankenphp_releases(client).await?,
        Runtime::Postgres => postgres_releases(client).await?,
        Runtime::Python => python_releases(client).await?,
        Runtime::Phppgadmin => phppgadmin_releases(client).await?,
        Runtime::Pgadmin => pgadmin_releases(client).await?,
        Runtime::Mongodb => mongodb_releases(client).await?,
        Runtime::Mongosh => mongosh_releases(client).await?,
        Runtime::Mongotools => mongotools_releases(client).await?,
        Runtime::Compass => compass_releases(client).await?,
        Runtime::Sqlite => sqlite_releases(client).await?,
        Runtime::Clickhouse => clickhouse_releases(client).await?,
        Runtime::Neo4j => neo4j_releases(client).await?,
        Runtime::Meilisearch => meilisearch_releases(client).await?,
        Runtime::Typesense => typesense_releases(client).await?,
        Runtime::Rustfs => rustfs_releases(client).await?,
        Runtime::Ollama => ollama_releases(client).await?,
        Runtime::Minio => own_builds(client, Runtime::Minio).await?,
        Runtime::Elasticsearch => elasticsearch_releases(client).await?,
        Runtime::Opensearch => opensearch_releases(client).await?,
        Runtime::N8n => n8n_releases(client).await?,
        Runtime::Rabbitmq => rabbitmq_releases(client).await?,
        Runtime::Erlang => erlang_releases(client).await?,
        Runtime::Go => go_releases(client).await?,
        Runtime::Rust => rust_releases(client).await?,
        Runtime::Maven => maven_releases(client).await?,
        Runtime::Gradle => gradle_releases(client).await?,
    };
    releases.sort_by(|a, b| version_key(&b.version).cmp(&version_key(&a.version)));
    Ok(releases)
}

/// Where LampRabit's own builds are published: Apache, Nginx, PHP, Redis and Memcached for
/// Linux and macOS, and Memcached for Windows (see build/linux/ and
/// .github/workflows/binaries.yml).
/// `LAMPRABIT_BINARIES_URL` points elsewhere, for testing new builds or a mirror.
const LINUX_BUILDS: &str = "https://github.com/Shivam-SV/LampRabit/releases/download/binaries/";

/// LampRabit's builds of one runtime for this OS and CPU, from the published manifest.
async fn own_builds(client: &reqwest::Client, runtime: Runtime) -> Result<Vec<Release>> {
    let base = std::env::var("LAMPRABIT_BINARIES_URL").unwrap_or_else(|_| LINUX_BUILDS.to_string());
    let base = format!("{}/", base.trim_end_matches('/'));
    let manifest: Value = client
        .get(format!("{base}manifest.json"))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| Error::msg(format!("Couldn't load the list of LampRabit's builds: {e}")))?
        .json()
        .await
        .map_err(http)?;
    let releases: Vec<Release> = manifest["releases"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["runtime"] == runtime.id() && e["os"] == std::env::consts::OS && e["arch"] == std::env::consts::ARCH)
        .filter_map(|e| {
            Some(Release {
                runtime,
                version: e["version"].as_str()?.to_string(),
                url: format!("{base}{}", e["file"].as_str()?),
                sha256: Some(e["sha256"].as_str()?.to_string()),
                sha512: None,
                md5: None,
                sha3_256: None,
                archive: Archive::TarGz,
                label: e["label"].as_str().map(str::to_string),
            })
        })
        .collect();
    if releases.is_empty() {
        return Err(Error::msg(format!(
            "There are no {} builds of {} for {} yet",
            std::env::consts::OS,
            runtime.id(),
            std::env::consts::ARCH
        )));
    }
    // Only the newest patch release of each line, like the Windows catalogs.
    let mut newest: Vec<Release> = Vec::new();
    for r in releases {
        match newest.iter_mut().find(|n| line(&n.version) == line(&r.version)) {
            Some(n) if version_key(&r.version) > version_key(&n.version) => *n = r,
            Some(_) => {}
            None => newest.push(r),
        }
    }
    Ok(newest)
}

/// windows.php.net lists the newest patch release of every branch, with checksums.
async fn php_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    if !cfg!(windows) {
        return own_builds(client, Runtime::Php).await;
    }
    const BASE: &str = "https://windows.php.net/downloads/releases/";
    let json: Value = client
        .get(format!("{BASE}releases.json"))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(http)?
        .json()
        .await
        .map_err(http)?;

    let mut out = Vec::new();
    for branch in json.as_object().into_iter().flat_map(|o| o.values()) {
        let Some(version) = branch["version"].as_str() else { continue };
        // Older branches are end-of-life and have known incompatibilities with current Apache builds.
        if version_key(version) < vec![8, 1] {
            continue;
        }
        // Thread-safe x64 build, required by mod_php.
        let Some(build) = branch
            .as_object()
            .and_then(|o| o.iter().find(|(k, _)| k.starts_with("ts-") && k.ends_with("-x64")))
            .map(|(_, v)| v)
        else {
            continue;
        };
        let Some(path) = build["zip"]["path"].as_str() else { continue };
        out.push(Release {
            runtime: Runtime::Php,
            version: version.to_string(),
            url: format!("{BASE}{path}"),
            sha256: build["zip"]["sha256"].as_str().map(str::to_string),
            sha512: None,
            md5: None,
            sha3_256: None,
            archive: Archive::Zip,
            // "8.4.27-1": the same PHP release rebuilt by windows.php.net (newer libraries).
            label: version.split_once('-').map(|(base, _)| format!("Rebuild of {base}")),
        });
    }
    Ok(out)
}

/// Apache Lounge only hosts its latest build, so the download page is scraped for it.
async fn apache_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    if !cfg!(windows) {
        return own_builds(client, Runtime::Apache).await;
    }
    const SITE: &str = "https://www.apachelounge.com";
    let page = client
        .get(format!("{SITE}/download/"))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(http)?
        .text()
        .await
        .map_err(http)?;

    let href = page
        .split("href=\"")
        .skip(1)
        .filter_map(|s| s.split('"').next())
        .find(|h| h.contains("/httpd-2.4.") && h.contains("Win64") && h.ends_with(".zip"))
        .ok_or_else(|| Error::msg("could not find an Apache build on apachelounge.com"))?;
    let url = if href.starts_with("http") { href.to_string() } else { format!("{SITE}{href}") };
    let file = url.rsplit('/').next().unwrap_or_default();
    // httpd-2.4.69-261002-Win64-VS18.zip -> 2.4.69
    let version = file
        .strip_prefix("httpd-")
        .and_then(|s| s.split('-').next())
        .ok_or_else(|| Error::msg(format!("unexpected Apache file name {file}")))?
        .to_string();

    // Checksums live next to the archive as "<file>.txt".
    let sha256 = match client.get(format!("{url}.txt")).send().await {
        Ok(r) if r.status().is_success() => r.text().await.ok().and_then(|t| {
            let mut lines = t.lines();
            lines.find(|l| l.starts_with("SHA256"))?;
            lines.next().map(|l| l.trim().to_ascii_lowercase())
        }),
        _ => None,
    };

    let label = file.split('-').find(|p| p.starts_with("VS")).map(|vs| vs.trim_end_matches(".zip").to_string());
    Ok(vec![Release { runtime: Runtime::Apache, version, url, sha256, sha512: None, md5: None, sha3_256: None, archive: Archive::Zip, label }])
}

/// The MariaDB REST API lists stable series and the files of their latest release.
async fn mariadb_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let (suffix, archive) = if cfg!(windows) {
        ("winx64.zip", Archive::Zip)
    } else if cfg!(target_os = "linux") {
        ("linux-systemd-x86_64.tar.gz", Archive::TarGz)
    } else {
        return Err(unsupported("MariaDB"));
    };
    const API: &str = "https://downloads.mariadb.org/rest-api/mariadb";

    let index: Value = client
        .get(format!("{API}/"))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(http)?
        .json()
        .await
        .map_err(http)?;
    let series: Vec<(String, Option<String>)> = index["major_releases"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["release_status"] == "Stable")
        .filter_map(|r| {
            let id = r["release_id"].as_str()?.to_string();
            let label = match r["release_support_type"].as_str() {
                Some("Long Term Support") => Some("LTS".to_string()),
                Some(other) => Some(other.to_string()),
                None => None,
            };
            Some((id, label))
        })
        .collect();

    let fetches = series.into_iter().map(|(id, label)| async move {
        let latest: Value = client.get(format!("{API}/{id}/latest/")).send().await.ok()?.json().await.ok()?;
        let release = latest["releases"].as_object()?.values().next()?;
        let file = release["files"]
            .as_array()?
            .iter()
            .find(|f| f["file_name"].as_str().is_some_and(|n| n.ends_with(suffix) && !n.contains("debug")))?;
        Some(Release {
            runtime: Runtime::Mariadb,
            version: release["release_id"].as_str()?.to_string(),
            url: file["file_download_url"].as_str()?.replacen("http://", "https://", 1),
            sha256: file["checksum"]["sha256sum"].as_str().map(str::to_string),
            sha512: None,
            md5: None,
            sha3_256: None,
            archive,
            label,
        })
    });
    Ok(futures_util::future::join_all(fetches).await.into_iter().flatten().collect())
}

/// nginx.org lists the mainline and stable Windows builds on its download page. It
/// publishes PGP signatures rather than checksums, so these downloads rely on HTTPS alone.
async fn nginx_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    if !cfg!(windows) {
        return own_builds(client, Runtime::Nginx).await;
    }
    const SITE: &str = "https://nginx.org";
    let page = client
        .get(format!("{SITE}/en/download.html"))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(http)?
        .text()
        .await
        .map_err(http)?;
    // In page order: mainline first, then stable, then legacy.
    let mut releases: Vec<Release> = Vec::new();
    for href in page.split("href=\"").skip(1).filter_map(|s| s.split('"').next()) {
        let Some(version) = href.strip_prefix("/download/nginx-").and_then(|v| v.strip_suffix(".zip")) else {
            continue;
        };
        if releases.iter().any(|r| r.version == version) {
            continue;
        }
        let label = match releases.len() {
            0 => Some("Mainline".to_string()),
            1 => Some("Stable".to_string()),
            _ => None,
        };
        releases.push(Release {
            runtime: Runtime::Nginx,
            version: version.to_string(),
            url: format!("{SITE}{href}"),
            sha256: None,
            sha512: None,
            md5: None,
            sha3_256: None,
            archive: Archive::Zip,
            label,
        });
        if releases.len() == 4 {
            break;
        }
    }
    if releases.is_empty() {
        return Err(Error::msg("could not find Nginx builds on nginx.org"));
    }
    Ok(releases)
}

async fn get_json(client: &reqwest::Client, url: &str) -> Result<Value> {
    let mut request = client.get(url);
    // GitHub's API wants its own media type (other hosts, npm's registry for one, refuse
    // it), and allows 60 anonymous requests an hour; a token raises that to 5000.
    if url.starts_with("https://api.github.com/") {
        request = request.header("Accept", "application/vnd.github+json");
        if let Some(token) = ["GITHUB_TOKEN", "GH_TOKEN"].iter().find_map(|v| std::env::var(v).ok().filter(|t| !t.is_empty())) {
            request = request.bearer_auth(token);
        }
    }
    request
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(http)?
        .json()
        .await
        .map_err(http)
}

/// Latest GitHub release of `repo`, picking the asset `pick` accepts. GitHub reports a
/// sha256 digest for each asset.
async fn github_latest(
    client: &reqwest::Client,
    runtime: Runtime,
    repo: &str,
    archive: Archive,
    pick: impl Fn(&str, &str) -> bool,
) -> Result<Vec<Release>> {
    let latest = get_json(client, &format!("https://api.github.com/repos/{repo}/releases/latest")).await?;
    let version = latest["tag_name"].as_str().unwrap_or_default().trim_start_matches('v').to_string();
    let asset = latest["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|a| a["name"].as_str().is_some_and(|n| pick(n, &version)))
        .ok_or_else(|| Error::msg(format!("no download for this platform in {repo} {version}")))?;
    Ok(vec![Release {
        runtime,
        url: asset["browser_download_url"].as_str().unwrap_or_default().to_string(),
        sha256: asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_string),
        sha512: None,
        md5: None,
        sha3_256: None,
        version,
        archive,
        label: None,
    }])
}

async fn mailpit_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let os = match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "darwin",
        "linux" => "linux",
        _ => return Err(unsupported("Mailpit")),
    };
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "amd64" };
    let (ext, archive) = if cfg!(windows) { ("zip", Archive::Zip) } else { ("tar.gz", Archive::TarGz) };
    let name = format!("mailpit-{os}-{arch}.{ext}");
    github_latest(client, Runtime::Mailpit, "axllent/mailpit", archive, |n, _| n == name).await
}

async fn caddy_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let os = match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "mac",
        "linux" => "linux",
        _ => return Err(unsupported("Caddy")),
    };
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "amd64" };
    let (ext, archive) = if cfg!(windows) { ("zip", Archive::Zip) } else { ("tar.gz", Archive::TarGz) };
    github_latest(client, Runtime::Caddy, "caddyserver/caddy", archive, |n, v| n == format!("caddy_{v}_{os}_{arch}.{ext}")).await
}

/// The newest Temurin JDK of every LTS line, plus the newest release when it isn't one.
/// Versions drop the build number ("21.0.5", not "21.0.5+11"), so folders and shims
/// read like the other runtimes.
async fn java_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let os = match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "mac",
        "linux" => "linux",
        _ => return Err(unsupported("Java")),
    };
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x64" };
    let archive = if cfg!(windows) { Archive::Zip } else { Archive::TarGz };
    let info = get_json(client, "https://api.adoptium.net/v3/info/available_releases").await?;
    let lts: Vec<u64> = info["available_lts_releases"].as_array().into_iter().flatten().filter_map(Value::as_u64).collect();
    let mut lines: Vec<u64> = lts.iter().copied().filter(|&v| v >= 8).collect();
    if let Some(newest) = info["most_recent_feature_release"].as_u64().filter(|v| !lts.contains(v)) {
        lines.push(newest);
    }
    let lts = &lts;
    let lookups = lines.into_iter().map(|line| async move {
        let url = format!("https://api.adoptium.net/v3/assets/latest/{line}/hotspot?architecture={arch}&image_type=jdk&os={os}&vendor=eclipse");
        let assets = get_json(client, &url).await.ok()?;
        let asset = assets.as_array()?.first()?;
        let package = &asset["binary"]["package"];
        let semver = asset["version"]["semver"].as_str()?;
        Some(Release {
            runtime: Runtime::Java,
            version: semver.split('+').next().unwrap_or(semver).to_string(),
            url: package["link"].as_str()?.to_string(),
            sha256: package["checksum"].as_str().map(str::to_string),
            sha512: None,
            md5: None,
            sha3_256: None,
            archive,
            label: Some(if lts.contains(&line) { "LTS".into() } else { "Latest".into() }),
        })
    });
    let releases: Vec<Release> = futures_util::future::join_all(lookups).await.into_iter().flatten().collect();
    if releases.is_empty() {
        return Err(Error::msg("Couldn't load the Java versions from Adoptium. Check your connection and try again."));
    }
    Ok(releases)
}

/// The newest release of each supported Tomcat line, from the Apache mirror's folder
/// listing, checked against the SHA-512 file published next to each archive.
async fn tomcat_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    const MIRROR: &str = "https://dlcdn.apache.org/tomcat";
    let (ext, archive) = if cfg!(windows) { ("zip", Archive::Zip) } else { ("tar.gz", Archive::TarGz) };
    let lookups = [(11, "Jakarta EE 11 · Java 17+"), (10, "Jakarta EE 10 · Java 11+"), (9, "Java EE 8 (javax) · Java 8+")].map(|(line, label)| async move {
        let listing = client.get(format!("{MIRROR}/tomcat-{line}/")).send().await.ok()?.error_for_status().ok()?.text().await.ok()?;
        let version = listing
            .split("href=\"v")
            .skip(1)
            .filter_map(|s| s.split('/').next())
            .filter(|v| v.split('.').all(|p| p.parse::<u32>().is_ok()))
            .max_by_key(|v| version_key(v))?
            .to_string();
        let url = format!("{MIRROR}/tomcat-{line}/v{version}/bin/apache-tomcat-{version}.{ext}");
        // "<hash> *apache-tomcat-x.y.z.zip"
        let sha512 = client.get(format!("{url}.sha512")).send().await.ok()?.error_for_status().ok()?.text().await.ok()?;
        let sha512 = sha512.split_whitespace().next()?.to_ascii_lowercase();
        Some(Release { runtime: Runtime::Tomcat, version, url, sha256: None, sha512: Some(sha512), md5: None, sha3_256: None, archive, label: Some(label.into()) })
    });
    let releases: Vec<Release> = futures_util::future::join_all(lookups).await.into_iter().flatten().collect();
    if releases.is_empty() {
        return Err(Error::msg("Couldn't load the Tomcat versions from dlcdn.apache.org. Check your connection and try again."));
    }
    Ok(releases)
}

/// FrankenPHP's releases: a static binary on Linux and Apple Silicon, a zip with PHP's
/// DLLs on Windows. There is no build for Intel Macs.
async fn frankenphp_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let arm = cfg!(target_arch = "aarch64");
    let (name, archive) = match std::env::consts::OS {
        "windows" if !arm => ("frankenphp-windows-x86_64.zip", Archive::Zip),
        "linux" if arm => ("frankenphp-linux-aarch64", Archive::File),
        "linux" => ("frankenphp-linux-x86_64", Archive::File),
        "macos" if arm => ("frankenphp-mac-arm64", Archive::File),
        "macos" => return Err(Error::msg("FrankenPHP has no build for Intel Macs. Use Caddy, Nginx or Apache with LampRabit's PHP instead.")),
        _ => return Err(unsupported("FrankenPHP")),
    };
    github_latest(client, Runtime::Frankenphp, "php/frankenphp", archive, |n, _| n == name).await
}

/// The newest build of each supported PostgreSQL major version (14 and up). The project
/// publishes one release per PostgreSQL version with an archive per platform, and GitHub
/// gives each a SHA-256.
async fn postgres_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let arm = cfg!(target_arch = "aarch64");
    let target = match (std::env::consts::OS, arm) {
        ("windows", false) => "x86_64-pc-windows-msvc",
        ("windows", true) => "aarch64-pc-windows-msvc",
        ("linux", false) => "x86_64-unknown-linux-gnu",
        ("linux", true) => "aarch64-unknown-linux-gnu",
        ("macos", false) => "x86_64-apple-darwin",
        ("macos", true) => "aarch64-apple-darwin",
        _ => return Err(unsupported("PostgreSQL")),
    };
    let (ext, archive) = if cfg!(windows) { ("zip", Archive::Zip) } else { ("tar.gz", Archive::TarGz) };
    let list = get_json(client, "https://api.github.com/repos/theseus-rs/postgresql-binaries/releases?per_page=100").await?;
    let mut newest: std::collections::BTreeMap<u32, Release> = Default::default();
    for release in list.as_array().into_iter().flatten().filter(|r| !r["prerelease"].as_bool().unwrap_or(false)) {
        let tag = release["tag_name"].as_str().unwrap_or_default();
        let Some(major) = tag.split('.').next().and_then(|m| m.parse::<u32>().ok()).filter(|&m| m >= 14) else { continue };
        let name = format!("postgresql-{tag}-{target}.{ext}");
        let Some(asset) = release["assets"].as_array().into_iter().flatten().find(|a| a["name"].as_str() == Some(name.as_str())) else { continue };
        if newest.get(&major).is_some_and(|r| version_key(&r.version) >= version_key(tag)) {
            continue;
        }
        newest.insert(major, Release {
            runtime: Runtime::Postgres,
            version: tag.to_string(),
            url: asset["browser_download_url"].as_str().unwrap_or_default().to_string(),
            sha256: asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_string),
            sha512: None,
            md5: None,
            sha3_256: None,
            archive,
            label: Some(format!("PostgreSQL {major}")),
        });
    }
    if newest.is_empty() {
        return Err(Error::msg("No PostgreSQL builds for this platform were found. Check your connection and try again."));
    }
    Ok(newest.into_values().collect())
}

/// The newest patch of every stable Python line from the latest python-build-standalone
/// release (its "install_only" archives: a plain Python folder, no debug files).
async fn python_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let arm = cfg!(target_arch = "aarch64");
    let target = match (std::env::consts::OS, arm) {
        ("windows", false) => "x86_64-pc-windows-msvc",
        ("windows", true) => "aarch64-pc-windows-msvc",
        ("linux", false) => "x86_64-unknown-linux-gnu",
        ("linux", true) => "aarch64-unknown-linux-gnu",
        ("macos", false) => "x86_64-apple-darwin",
        ("macos", true) => "aarch64-apple-darwin",
        _ => return Err(unsupported("Python")),
    };
    let latest = get_json(client, "https://api.github.com/repos/astral-sh/python-build-standalone/releases/latest").await?;
    let suffix = format!("-{target}-install_only.tar.gz");
    let mut newest: std::collections::BTreeMap<String, Release> = Default::default();
    for asset in latest["assets"].as_array().into_iter().flatten() {
        let name = asset["name"].as_str().unwrap_or_default();
        // cpython-3.13.18+20261026-x86_64-pc-windows-msvc-install_only.tar.gz
        let Some(version) = name.strip_prefix("cpython-").and_then(|n| n.strip_suffix(&suffix)).and_then(|n| n.split('+').next()) else { continue };
        // Stable releases only (no 3.16.0a1).
        if !version.split('.').all(|p| p.parse::<u32>().is_ok()) {
            continue;
        }
        let line = line(version);
        if newest.get(&line).is_some_and(|r| version_key(&r.version) >= version_key(version)) {
            continue;
        }
        newest.insert(line, Release {
            runtime: Runtime::Python,
            version: version.to_string(),
            url: asset["browser_download_url"].as_str().unwrap_or_default().to_string(),
            sha256: asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_string),
            sha512: None,
            md5: None,
            sha3_256: None,
            archive: Archive::TarGz,
            label: None,
        });
    }
    if newest.is_empty() {
        return Err(Error::msg("No Python builds for this platform were found. Check your connection and try again."));
    }
    Ok(newest.into_values().collect())
}

/// phpPgAdmin from ReimuHakurei's fork, which supports PHP 8 and current PostgreSQL.
/// Its releases are tags without uploaded files, so the download is GitHub's source zip,
/// checked by HTTPS only.
async fn phppgadmin_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let latest = get_json(client, "https://api.github.com/repos/ReimuHakurei/phpPgAdmin/releases/latest").await?;
    let tag = latest["tag_name"].as_str().ok_or_else(|| Error::msg("Couldn't read phpPgAdmin's latest release"))?;
    let version = tag.trim_start_matches('v').trim_end_matches("-mod").to_string();
    Ok(vec![Release {
        runtime: Runtime::Phppgadmin,
        version,
        url: format!("https://github.com/ReimuHakurei/phpPgAdmin/archive/refs/tags/{tag}.zip"),
        sha256: None,
        sha512: None,
        md5: None,
        sha3_256: None,
        archive: Archive::Zip,
        label: Some("PHP 8 fork".into()),
    }])
}

/// pgAdmin 4 releases from PyPI: one wheel for every platform, with PyPI's SHA-256. The
/// wheel is saved as is; `setup_pgadmin` installs it with its dependencies.
async fn pgadmin_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let info = get_json(client, "https://pypi.org/pypi/pgadmin4/json").await?;
    let version = info["info"]["version"].as_str().ok_or_else(|| Error::msg("Couldn't read pgAdmin's latest release from PyPI"))?.to_string();
    let wheel = info["urls"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|u| u["filename"].as_str().is_some_and(|f| f.ends_with(".whl")))
        .ok_or_else(|| Error::msg("PyPI has no pgAdmin wheel for the latest release"))?;
    Ok(vec![Release {
        runtime: Runtime::Pgadmin,
        version,
        url: wheel["url"].as_str().unwrap_or_default().to_string(),
        sha256: wheel["digests"]["sha256"].as_str().map(str::to_string),
        sha512: None,
        md5: None,
        sha3_256: None,
        archive: Archive::File,
        label: None,
    }])
}

/// The Python a pgAdmin install runs on: the newest installed 3.10–3.14 (its
/// dependencies ship binary wheels for those), preferring the active version.
pub fn pgadmin_python(settings: &crate::config::Settings, layout: &Layout) -> Result<PathBuf> {
    let usable = |v: &str| version_key(v).get(..2).is_some_and(|mm| mm[0] == 3 && (10..=14).contains(&mm[1]));
    let mut versions = installed(layout, Runtime::Python);
    versions.retain(|v| usable(v));
    versions.sort_by_key(|v| std::cmp::Reverse(version_key(v)));
    let version = settings.versions.get("python").filter(|v| usable(v)).cloned().or_else(|| versions.into_iter().next())
        .ok_or_else(|| Error::msg("pgAdmin runs on Python: install Python 3.13 (Services → Languages → Python) first."))?;
    Ok(layout.bin("python", &version).join(Runtime::Python.marker()))
}

/// A virtual environment's Python.
pub fn venv_python(venv: &Path) -> PathBuf {
    if cfg!(windows) { venv.join("Scripts").join("python.exe") } else { venv.join("bin").join("python3") }
}

/// pgAdmin's package folder inside its virtual environment (Lib/site-packages/pgadmin4 on
/// Windows, lib/pythonX.Y/site-packages/pgadmin4 elsewhere).
pub fn pgadmin_package(home: &Path) -> Option<PathBuf> {
    let venv = home.join("venv");
    if cfg!(windows) {
        return Some(venv.join("Lib").join("site-packages").join("pgadmin4")).filter(|p| p.is_dir());
    }
    std::fs::read_dir(venv.join("lib")).ok()?.flatten().map(|e| e.path().join("site-packages").join("pgadmin4")).find(|p| p.is_dir())
}

/// Runs a setup command, failing with the last line it printed to stderr.
async fn run_quiet(program: &Path, args: &[String], cwd: &Path) -> Result<()> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args).current_dir(cwd).stdin(std::process::Stdio::null());
    crate::procgroup::prepare(&mut cmd);
    let out = cmd.output().await?;
    if out.status.success() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&out.stderr);
    let last = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
    Err(Error::msg(format!("{} failed: {last}", program.display())))
}

/// Installs a downloaded pgAdmin wheel into its own virtual environment, with its
/// dependencies from PyPI. Leaves nothing behind when it fails.
pub async fn setup_pgadmin(app: &AppHandle, settings: &crate::config::Settings, layout: &Layout, version: &str) -> Result<()> {
    let home = layout.bin("pgadmin", version);
    let result = async {
        let python = pgadmin_python(settings, layout)?;
        let _ = app.emit("install-progress", InstallProgress { runtime: Runtime::Pgadmin, version: version.into(), stage: "extracting", downloaded: 0, total: None });
        run_quiet(&python, &["-m".into(), "venv".into(), home.join("venv").display().to_string()], &home).await?;
        // pip only accepts a wheel under its real file name.
        let wheel = home.join(format!("pgadmin4-{version}-py3-none-any.whl"));
        std::fs::rename(home.join("installed.txt"), &wheel)?;
        let pip = |extra: &[&str]| {
            let mut args: Vec<String> = vec!["-m".into(), "pip".into(), "install".into(), "--disable-pip-version-check".into(), "--no-input".into()];
            args.extend(extra.iter().map(|s| s.to_string()));
            args
        };
        run_quiet(&venv_python(&home.join("venv")), &pip(&[&wheel.display().to_string()]), &home).await?;
        std::fs::remove_file(&wheel)?;
        std::fs::write(home.join("installed.txt"), format!("pgAdmin {version}\n"))?;
        Ok::<_, Error>(())
    }
    .await;
    if let Err(e) = &result {
        let _ = std::fs::remove_dir_all(&home);
        return Err(Error::msg(format!("Installing pgAdmin's Python packages failed: {e}")));
    }
    result
}

/// MongoDB's Linux builds are per distribution; the one for this machine, from
/// /etc/os-release. Releases of a distro between two supported ones use the older build.
fn mongodb_linux_target() -> Result<String> {
    mongodb_target_for(&std::fs::read_to_string("/etc/os-release").unwrap_or_default())
}

fn mongodb_target_for(text: &str) -> Result<String> {
    let field = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(&format!("{key}=")))
            .map(|v| v.trim_matches('"').to_string())
            .unwrap_or_default()
    };
    let (id, like, version) = (field("ID"), field("ID_LIKE"), field("VERSION_ID"));
    let major: u32 = version.split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0);
    let pick = |supported: &[(u32, &str)], have: u32| supported.iter().rev().find(|(v, _)| *v <= have).map(|(_, t)| t.to_string());
    let ubuntu = [(2204, "ubuntu2204"), (2404, "ubuntu2404"), (2604, "ubuntu2604")];
    let found = match id.as_str() {
        "ubuntu" => pick(&ubuntu, version.replace('.', "").parse().unwrap_or(0)),
        "debian" => pick(&[(12, "debian12"), (13, "debian13")], major),
        "rhel" | "centos" | "rocky" | "almalinux" | "ol" => pick(&[(8, "rhel8"), (9, "rhel93"), (10, "rhel10")], major),
        "amzn" if major >= 2023 => Some("amazon2023".into()),
        "sles" | "opensuse-leap" => pick(&[(15, "suse15"), (16, "suse16")], major),
        // Ubuntu-based (Mint, Pop!_OS…): the Ubuntu release they're built on.
        _ if like.contains("ubuntu") => match field("UBUNTU_CODENAME").as_str() {
            "jammy" => Some("ubuntu2204".into()),
            _ => Some("ubuntu2404".into()),
        },
        _ => None,
    };
    found.ok_or_else(|| {
        let name = field("PRETTY_NAME");
        Error::msg(format!(
            "MongoDB publishes Linux builds for Ubuntu, Debian, RHEL and its rebuilds, Amazon Linux and SUSE; {} isn't one of them.",
            if name.is_empty() { "this distribution".to_string() } else { name }
        ))
    })
}

/// The newest release of each MongoDB major version from 7 on, from MongoDB's own list of
/// downloads (with SHA-256s): the Community ("base", on Linux "targeted") editions.
async fn mongodb_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let arm = cfg!(target_arch = "aarch64");
    let (target, arch, edition) = match std::env::consts::OS {
        "windows" if !arm => ("windows".to_string(), "x86_64", "base"),
        "macos" => ("macos".to_string(), if arm { "arm64" } else { "x86_64" }, "base"),
        "linux" => (mongodb_linux_target()?, if arm { "aarch64" } else { "x86_64" }, "targeted"),
        _ => return Err(unsupported("MongoDB")),
    };
    let full = get_json(client, "https://downloads.mongodb.org/full.json").await?;
    let mut newest: std::collections::BTreeMap<u32, Release> = Default::default();
    for v in full["versions"].as_array().into_iter().flatten().filter(|v| v["production_release"].as_bool() == Some(true)) {
        let version = v["version"].as_str().unwrap_or_default();
        let Some(major) = version.split('.').next().and_then(|m| m.parse::<u32>().ok()).filter(|&m| m >= 7) else { continue };
        if newest.get(&major).is_some_and(|r| version_key(&r.version) >= version_key(version)) {
            continue;
        }
        let Some(download) = v["downloads"].as_array().into_iter().flatten().find(|d| {
            d["target"].as_str() == Some(target.as_str()) && d["arch"].as_str() == Some(arch) && d["edition"].as_str() == Some(edition)
        }) else {
            continue;
        };
        let url = download["archive"]["url"].as_str().unwrap_or_default().to_string();
        newest.insert(major, Release {
            runtime: Runtime::Mongodb,
            version: version.to_string(),
            archive: if url.ends_with(".zip") { Archive::Zip } else { Archive::TarGz },
            sha256: download["archive"]["sha256"].as_str().map(str::to_string),
            sha512: None,
            md5: None,
            sha3_256: None,
            url,
            label: Some(if v["lts_release"].as_bool() == Some(true) { "LTS".into() } else { format!("MongoDB {major}") }),
        });
    }
    if newest.is_empty() {
        return Err(Error::msg("MongoDB has no build for this platform. Check your connection and try again."));
    }
    Ok(newest.into_values().collect())
}

async fn mongosh_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
    let (platform, ext, archive) = match std::env::consts::OS {
        "windows" => ("win32", "zip", Archive::Zip),
        "macos" => ("darwin", "zip", Archive::Zip),
        "linux" => ("linux", "tgz", Archive::TarGz),
        _ => return Err(unsupported("mongosh")),
    };
    github_latest(client, Runtime::Mongosh, "mongodb-js/mongosh", archive, |n, v| n == format!("mongosh-{v}-{platform}-{arch}.{ext}")).await
}

/// The newest MongoDB Database Tools. They're built against old distributions (Ubuntu
/// 18.04, RHEL 8), which also run on newer ones.
async fn mongotools_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let arm = cfg!(target_arch = "aarch64");
    let (name, arch) = match std::env::consts::OS {
        "windows" if !arm => ("windows", "x86_64"),
        "macos" => ("macos", "x86_64"),
        "linux" if arm => ("ubuntu1804", "arm64"),
        "linux" => ("ubuntu1804", "x86_64"),
        _ => return Err(unsupported("MongoDB Database Tools")),
    };
    let full = get_json(client, "https://downloads.mongodb.org/tools/db/full.json").await?;
    let newest = full["versions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|v| v["version"].as_str().is_some_and(|s| !s.contains('-')))
        .max_by_key(|v| version_key(v["version"].as_str().unwrap_or_default()))
        .ok_or_else(|| Error::msg("Couldn't read the MongoDB Database Tools releases"))?;
    let download = newest["downloads"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|d| d["name"].as_str() == Some(name) && d["arch"].as_str() == Some(arch))
        .ok_or_else(|| unsupported("MongoDB Database Tools"))?;
    let url = download["archive"]["url"].as_str().unwrap_or_default().to_string();
    Ok(vec![Release {
        runtime: Runtime::Mongotools,
        version: newest["version"].as_str().unwrap_or_default().to_string(),
        archive: if url.ends_with(".zip") { Archive::Zip } else { Archive::TarGz },
        sha256: download["archive"]["sha256"].as_str().map(str::to_string),
        sha512: None,
        md5: None,
        sha3_256: None,
        url,
        label: None,
    }])
}

async fn compass_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
    let (name, archive) = match std::env::consts::OS {
        "windows" => ("win32-x64.zip".to_string(), Archive::Zip),
        "macos" => (format!("darwin-{arch}.zip"), Archive::Zip),
        "linux" => ("linux-x64.tar.gz".to_string(), Archive::TarGz),
        _ => return Err(unsupported("MongoDB Compass")),
    };
    github_latest(client, Runtime::Compass, "mongodb-js/compass", archive, |n, v| n == format!("mongodb-compass-{v}-{name}")).await
}

/// The sqlite3 shell from sqlite.org's download page, whose comment block lists every file
/// with its SHA3-256.
async fn sqlite_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let platform = match (std::env::consts::OS, cfg!(target_arch = "aarch64")) {
        ("windows", false) => "win-x64",
        ("windows", true) => "win-arm64",
        ("linux", false) => "linux-x64",
        ("macos", false) => "osx-x64",
        ("macos", true) => "osx-arm64",
        _ => return Err(unsupported("SQLite")),
    };
    let page = client.get("https://www.sqlite.org/download.html").send().await.and_then(|r| r.error_for_status()).map_err(http)?.text().await.map_err(http)?;
    // PRODUCT,3.54.1,2026/sqlite-tools-win-x64-3540100.zip,6984432,<sha3>
    let row = page
        .lines()
        .filter_map(|l| l.strip_prefix("PRODUCT,"))
        .map(|l| l.split(',').collect::<Vec<_>>())
        .find(|f| f.len() == 4 && f[1].contains(&format!("sqlite-tools-{platform}-")))
        .ok_or_else(|| unsupported("SQLite"))?;
    Ok(vec![Release {
        runtime: Runtime::Sqlite,
        version: row[0].to_string(),
        url: format!("https://www.sqlite.org/{}", row[1]),
        sha256: None,
        sha512: None,
        md5: None,
        sha3_256: Some(row[3].trim().to_string()),
        archive: Archive::Zip,
        label: None,
    }])
}

/// The newest stable (or LTS) ClickHouse: a static build for Linux, a single binary for
/// macOS. ClickHouse has no Windows build.
async fn clickhouse_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let arm = cfg!(target_arch = "aarch64");
    if cfg!(windows) {
        return Err(Error::msg("ClickHouse has no Windows build. Run it in WSL (Linux), where LampRabit's Linux version can install it."));
    }
    let list = get_json(client, "https://api.github.com/repos/ClickHouse/ClickHouse/releases?per_page=30").await?;
    let release = list
        .as_array()
        .into_iter()
        .flatten()
        .find(|r| r["tag_name"].as_str().is_some_and(|t| t.ends_with("-stable") || t.ends_with("-lts")))
        .ok_or_else(|| Error::msg("Couldn't find a stable ClickHouse release"))?;
    let tag = release["tag_name"].as_str().unwrap_or_default();
    let version = tag.trim_start_matches('v').rsplit_once('-').map_or(tag, |(v, _)| v).to_string();
    let (name, archive) = match std::env::consts::OS {
        "macos" if arm => ("clickhouse-macos-aarch64".to_string(), Archive::File),
        "macos" => ("clickhouse-macos".to_string(), Archive::File),
        _ => (format!("clickhouse-common-static-{version}-{}.tgz", if arm { "arm64" } else { "amd64" }), Archive::TarGz),
    };
    let asset = release["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|a| a["name"].as_str() == Some(name.as_str()))
        .ok_or_else(|| unsupported("ClickHouse"))?;
    Ok(vec![Release {
        runtime: Runtime::Clickhouse,
        version,
        url: asset["browser_download_url"].as_str().unwrap_or_default().to_string(),
        sha256: asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_string),
        sha512: None,
        md5: None,
        sha3_256: None,
        archive,
        label: Some(if tag.ends_with("-lts") { "LTS".into() } else { "Stable".into() }),
    }])
}

/// The newest Neo4j Community release (calendar versions like 2026.10.1, tagged in
/// neo4j/neo4j), from dist.neo4j.org with the SHA-256 published next to it.
async fn neo4j_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let tags = get_json(client, "https://api.github.com/repos/neo4j/neo4j/tags?per_page=30").await?;
    let version = tags
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| t["name"].as_str())
        .filter(|n| n.split('.').count() == 3 && n.split('.').all(|p| p.parse::<u32>().is_ok()))
        .max_by_key(|n| version_key(n))
        .ok_or_else(|| Error::msg("Couldn't find Neo4j's latest release"))?
        .to_string();
    let (suffix, archive) = if cfg!(windows) { ("windows.zip", Archive::Zip) } else { ("unix.tar.gz", Archive::TarGz) };
    let url = format!("https://dist.neo4j.org/neo4j-community-{version}-{suffix}");
    let sha256 = client.get(format!("{url}.sha256")).send().await.ok().filter(|r| r.status().is_success());
    let sha256 = match sha256 {
        Some(r) => r.text().await.ok().and_then(|t| t.split_whitespace().next().map(str::to_ascii_lowercase)),
        None => None,
    };
    Ok(vec![Release { runtime: Runtime::Neo4j, version, url, sha256, sha512: None, md5: None, sha3_256: None, archive, label: Some("Community".into()) }])
}

/// Meilisearch's Community edition (not "enterprise"): one binary per platform.
async fn meilisearch_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let name = match (std::env::consts::OS, cfg!(target_arch = "aarch64")) {
        ("windows", false) => "meilisearch-windows-amd64.exe",
        ("linux", false) => "meilisearch-linux-amd64",
        ("linux", true) => "meilisearch-linux-aarch64",
        ("macos", false) => "meilisearch-macos-amd64",
        ("macos", true) => "meilisearch-macos-apple-silicon",
        _ => return Err(unsupported("Meilisearch")),
    };
    github_latest(client, Runtime::Meilisearch, "meilisearch/meilisearch", Archive::File, |n, _| n == name).await
}

/// Typesense's server from dl.typesense.org (its GitHub releases have no files). No
/// checksums are published, so HTTPS covers the download. There's no Windows build.
async fn typesense_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    if cfg!(windows) {
        return Err(Error::msg("Typesense has no Windows build. Use Meilisearch, or run Typesense in WSL (Linux), where LampRabit's Linux version can install it."));
    }
    let latest = get_json(client, "https://api.github.com/repos/typesense/typesense/releases/latest").await?;
    let version = latest["tag_name"].as_str().unwrap_or_default().trim_start_matches('v').to_string();
    if version.is_empty() {
        return Err(Error::msg("Couldn't find Typesense's latest release"));
    }
    let platform = match (std::env::consts::OS, cfg!(target_arch = "aarch64")) {
        ("linux", false) => "linux-amd64",
        ("linux", true) => "linux-arm64",
        ("macos", false) => "darwin-amd64",
        ("macos", true) => "darwin-arm64",
        _ => return Err(unsupported("Typesense")),
    };
    Ok(vec![Release {
        runtime: Runtime::Typesense,
        url: format!("https://dl.typesense.org/releases/{version}/typesense-server-{version}-{platform}.tar.gz"),
        version,
        sha256: None,
        sha512: None,
        md5: None,
        sha3_256: None,
        archive: Archive::TarGz,
        label: None,
    }])
}

async fn rustfs_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let name = match (std::env::consts::OS, cfg!(target_arch = "aarch64")) {
        ("windows", false) => "windows-x86_64",
        ("linux", false) => "linux-x86_64-gnu",
        ("linux", true) => "linux-aarch64-gnu",
        ("macos", true) => "macos-aarch64",
        ("macos", false) => return Err(Error::msg("RustFS has no build for Intel Macs.")),
        _ => return Err(unsupported("RustFS")),
    };
    github_latest(client, Runtime::Rustfs, "rustfs/rustfs", Archive::Zip, |n, v| n == format!("rustfs-{name}-v{v}.zip")).await
}

/// Ollama's own builds (CPU and NVIDIA support; large: about 1.5 GB on Windows and Linux).
async fn ollama_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let (name, archive) = match (std::env::consts::OS, cfg!(target_arch = "aarch64")) {
        ("windows", false) => ("ollama-windows-amd64.zip", Archive::Zip),
        ("windows", true) => ("ollama-windows-arm64.zip", Archive::Zip),
        ("linux", false) => ("ollama-linux-amd64.tar.zst", Archive::TarZst),
        ("linux", true) => ("ollama-linux-arm64.tar.zst", Archive::TarZst),
        ("macos", _) => ("ollama-darwin.tgz", Archive::TarGz),
        _ => return Err(unsupported("Ollama")),
    };
    let mut releases = github_latest(client, Runtime::Ollama, "ollama/ollama", archive, |n, _| n == name).await?;
    for r in &mut releases {
        r.label = Some("Large download".into());
    }
    Ok(releases)
}

/// The newest release of each major version from GitHub release tags ("v9.5.5"), at least `min`.
async fn newest_per_major(client: &reqwest::Client, repo: &str, min: u32) -> Result<Vec<String>> {
    let list = get_json(client, &format!("https://api.github.com/repos/{repo}/releases?per_page=50")).await?;
    let mut newest: std::collections::BTreeMap<u32, String> = Default::default();
    for r in list.as_array().into_iter().flatten().filter(|r| !r["prerelease"].as_bool().unwrap_or(false) && !r["draft"].as_bool().unwrap_or(false)) {
        let version = r["tag_name"].as_str().unwrap_or_default().trim_start_matches('v').to_string();
        if !version.split('.').all(|p| p.parse::<u32>().is_ok()) {
            continue;
        }
        let Some(major) = version.split('.').next().and_then(|m| m.parse::<u32>().ok()).filter(|&m| m >= min) else { continue };
        if newest.get(&major).is_none_or(|v| version_key(&version) > version_key(v)) {
            newest.insert(major, version);
        }
    }
    Ok(newest.into_values().collect())
}

/// "<hash>  <file>" files published next to archives.
async fn published_hash(client: &reqwest::Client, url: &str) -> Option<String> {
    let r = client.get(url).send().await.ok()?.error_for_status().ok()?;
    r.text().await.ok()?.split_whitespace().next().map(str::to_ascii_lowercase)
}

/// Elasticsearch 8 and 9 from Elastic's artifact server, with the SHA-512 next to each.
async fn elasticsearch_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let (platform, ext, archive) = match (std::env::consts::OS, cfg!(target_arch = "aarch64")) {
        ("windows", false) => ("windows-x86_64", "zip", Archive::Zip),
        ("linux", false) => ("linux-x86_64", "tar.gz", Archive::TarGz),
        ("linux", true) => ("linux-aarch64", "tar.gz", Archive::TarGz),
        ("macos", false) => ("darwin-x86_64", "tar.gz", Archive::TarGz),
        ("macos", true) => ("darwin-aarch64", "tar.gz", Archive::TarGz),
        _ => return Err(unsupported("Elasticsearch")),
    };
    let mut out = Vec::new();
    for version in newest_per_major(client, "elastic/elasticsearch", 8).await? {
        let url = format!("https://artifacts.elastic.co/downloads/elasticsearch/elasticsearch-{version}-{platform}.{ext}");
        let sha512 = published_hash(client, &format!("{url}.sha512")).await;
        out.push(Release { runtime: Runtime::Elasticsearch, version, url, sha256: None, sha512, md5: None, sha3_256: None, archive, label: Some("1 GB+ RAM".into()) });
    }
    Ok(out)
}

/// OpenSearch 2 and 3 from its artifact server. It publishes no macOS build.
async fn opensearch_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let (platform, ext, archive) = match (std::env::consts::OS, cfg!(target_arch = "aarch64")) {
        ("windows", false) => ("windows-x64", "zip", Archive::Zip),
        ("linux", false) => ("linux-x64", "tar.gz", Archive::TarGz),
        ("linux", true) => ("linux-arm64", "tar.gz", Archive::TarGz),
        ("macos", _) => return Err(Error::msg("OpenSearch publishes no macOS build. Use Elasticsearch, which has one.")),
        _ => return Err(unsupported("OpenSearch")),
    };
    let mut out = Vec::new();
    for version in newest_per_major(client, "opensearch-project/OpenSearch", 2).await? {
        let url = format!("https://artifacts.opensearch.org/releases/bundle/opensearch/{version}/opensearch-{version}-{platform}.{ext}");
        let sha512 = published_hash(client, &format!("{url}.sha512")).await;
        out.push(Release { runtime: Runtime::Opensearch, version, url, sha256: None, sha512, md5: None, sha3_256: None, archive, label: Some("1 GB+ RAM".into()) });
    }
    Ok(out)
}

/// n8n's latest release from npm. The package tarball is saved as the marker file;
/// `setup_n8n` installs it with its dependencies (npm checks their integrity itself).
async fn n8n_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let latest = get_json(client, "https://registry.npmjs.org/n8n/latest").await?;
    let version = latest["version"].as_str().ok_or_else(|| Error::msg("Couldn't read n8n's latest release from npm"))?.to_string();
    let node = latest["engines"]["node"].as_str().unwrap_or("").to_string();
    Ok(vec![Release {
        runtime: Runtime::N8n,
        url: latest["dist"]["tarball"].as_str().unwrap_or_default().to_string(),
        version,
        sha256: None,
        sha512: None,
        md5: None,
        sha3_256: None,
        archive: Archive::File,
        label: (!node.is_empty()).then(|| format!("Node {node}")),
    }])
}

/// The newest RabbitMQ: the Windows zip, or the generic Unix build (which runs on the
/// system's Erlang).
async fn rabbitmq_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let (archive, ext) = if cfg!(windows) { (Archive::Zip, "zip") } else { (Archive::TarXz, "tar.xz") };
    let platform = if cfg!(windows) { "windows" } else { "generic-unix" };
    github_latest(client, Runtime::Rabbitmq, "rabbitmq/rabbitmq-server", archive, |n, v| n == format!("rabbitmq-server-{platform}-{v}.{ext}")).await
}

/// Erlang/OTP for RabbitMQ: the newest of the two latest major versions, from the
/// official Windows zips. Linux and macOS have no official builds.
async fn erlang_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    if !cfg!(windows) {
        return Err(Error::msg(
            "Erlang has no official Linux or macOS builds. Install it with your system's packages (sudo apt install erlang-nox, sudo dnf install erlang, or brew install erlang); LampRabit's RabbitMQ uses it.",
        ));
    }
    let list = get_json(client, "https://api.github.com/repos/erlang/otp/releases?per_page=40").await?;
    let mut newest: std::collections::BTreeMap<u32, Release> = Default::default();
    for r in list.as_array().into_iter().flatten().filter(|r| !r["prerelease"].as_bool().unwrap_or(false)) {
        let version = r["tag_name"].as_str().unwrap_or_default().trim_start_matches("OTP-").to_string();
        let Some(major) = version.split('.').next().and_then(|m| m.parse::<u32>().ok()) else { continue };
        if newest.get(&major).is_some_and(|n| version_key(&n.version) >= version_key(&version)) {
            continue;
        }
        let name = format!("otp_win64_{version}.zip");
        let Some(asset) = r["assets"].as_array().into_iter().flatten().find(|a| a["name"].as_str() == Some(name.as_str())) else { continue };
        newest.insert(major, Release {
            runtime: Runtime::Erlang,
            url: asset["browser_download_url"].as_str().unwrap_or_default().to_string(),
            sha256: asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_string),
            version,
            sha512: None,
            md5: None,
            sha3_256: None,
            archive: Archive::Zip,
            label: Some(format!("OTP {major}")),
        });
    }
    let mut releases: Vec<Release> = newest.into_values().rev().take(2).collect();
    releases.reverse();
    if releases.is_empty() {
        return Err(Error::msg("Couldn't find Erlang's Windows builds. Check your connection and try again."));
    }
    Ok(releases)
}

/// The newest Maven 4 and Maven 3 from Apache's mirror, SHA-512 checked like Tomcat. The
/// tarball keeps the Unix scripts executable; Windows gets the zip.
async fn maven_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    const MIRROR: &str = "https://dlcdn.apache.org/maven";
    let (ext, archive) = if cfg!(windows) { ("zip", Archive::Zip) } else { ("tar.gz", Archive::TarGz) };
    let lookups = [(4, "Java 17+"), (3, "Java 8+")].map(|(line, label)| async move {
        let listing = client.get(format!("{MIRROR}/maven-{line}/")).send().await.ok()?.error_for_status().ok()?.text().await.ok()?;
        // Only final releases: 4.0.0-rc-4 and the like are skipped.
        let version = listing
            .split("href=\"")
            .skip(1)
            .filter_map(|s| s.split('/').next())
            .filter(|v| v.starts_with(&format!("{line}.")) && v.split('.').all(|p| p.parse::<u32>().is_ok()))
            .max_by_key(|v| version_key(v))?
            .to_string();
        let url = format!("{MIRROR}/maven-{line}/{version}/binaries/apache-maven-{version}-bin.{ext}");
        let sha512 = client.get(format!("{url}.sha512")).send().await.ok()?.error_for_status().ok()?.text().await.ok()?;
        let sha512 = sha512.split_whitespace().next()?.to_ascii_lowercase();
        Some(Release { runtime: Runtime::Maven, version, url, sha256: None, sha512: Some(sha512), md5: None, sha3_256: None, archive, label: Some(label.into()) })
    });
    let releases: Vec<Release> = futures_util::future::join_all(lookups).await.into_iter().flatten().collect();
    if releases.is_empty() {
        return Err(Error::msg("Couldn't load the Maven versions from dlcdn.apache.org. Check your connection and try again."));
    }
    Ok(releases)
}

/// The newest final release of Gradle's two latest major versions, from services.gradle.org,
/// with the SHA-256 Gradle publishes for each distribution (one zip for every OS).
async fn gradle_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let list = get_json(client, "https://services.gradle.org/versions/all").await?;
    let mut newest: std::collections::BTreeMap<u32, (String, String, String)> = Default::default();
    for v in list.as_array().into_iter().flatten() {
        let final_release = !v["snapshot"].as_bool().unwrap_or(true)
            && !v["nightly"].as_bool().unwrap_or(true)
            && !v["broken"].as_bool().unwrap_or(true)
            && v["rcFor"].as_str().unwrap_or_default().is_empty()
            && v["milestoneFor"].as_str().unwrap_or_default().is_empty();
        let (Some(version), Some(url), Some(sum)) = (v["version"].as_str(), v["downloadUrl"].as_str(), v["checksumUrl"].as_str()) else { continue };
        let Some(major) = version.split('.').next().and_then(|m| m.parse::<u32>().ok()) else { continue };
        if final_release && newest.get(&major).is_none_or(|(n, _, _)| version_key(n) < version_key(version)) {
            newest.insert(major, (version.to_string(), url.to_string(), sum.to_string()));
        }
    }
    let mut releases = Vec::new();
    for (version, url, sum_url) in newest.into_values().rev().take(2) {
        let sha256 = client.get(&sum_url).send().await.ok().and_then(|r| r.error_for_status().ok());
        let Some(sha256) = (match sha256 {
            Some(r) => r.text().await.ok().and_then(|t| t.split_whitespace().next().map(str::to_ascii_lowercase)),
            None => None,
        }) else {
            continue;
        };
        releases.push(Release { runtime: Runtime::Gradle, version, url, sha256: Some(sha256), sha512: None, md5: None, sha3_256: None, archive: Archive::Zip, label: None });
    }
    if releases.is_empty() {
        return Err(Error::msg("Couldn't load the Gradle versions from services.gradle.org. Check your connection and try again."));
    }
    Ok(releases)
}

/// The supported Go releases (the newest two lines) from go.dev, with their SHA-256s.
async fn go_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let os = match std::env::consts::OS {
        "windows" => "windows",
        "linux" => "linux",
        "macos" => "darwin",
        _ => return Err(unsupported("Go")),
    };
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "amd64" };
    let list = get_json(client, "https://go.dev/dl/?mode=json").await?;
    let releases: Vec<Release> = list
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["stable"].as_bool() == Some(true))
        .filter_map(|r| {
            let file = r["files"].as_array()?.iter().find(|f| f["os"] == os && f["arch"] == arch && f["kind"] == "archive")?;
            let name = file["filename"].as_str()?;
            Some(Release {
                runtime: Runtime::Go,
                version: r["version"].as_str()?.trim_start_matches("go").to_string(),
                url: format!("https://go.dev/dl/{name}"),
                sha256: file["sha256"].as_str().map(str::to_string),
                sha512: None,
                md5: None,
                sha3_256: None,
                archive: if name.ends_with(".zip") { Archive::Zip } else { Archive::TarGz },
                label: None,
            })
        })
        .collect();
    if releases.is_empty() {
        return Err(unsupported("Go"));
    }
    Ok(releases)
}

/// The host rustup installs for: on Windows the GNU toolchain, which links with its own
/// MinGW tools instead of needing Visual Studio's C++ Build Tools.
fn rust_host() -> Option<&'static str> {
    Some(match (std::env::consts::OS, cfg!(target_arch = "aarch64")) {
        ("windows", false) => "x86_64-pc-windows-gnu",
        ("windows", true) => "aarch64-pc-windows-gnullvm",
        ("linux", false) => "x86_64-unknown-linux-gnu",
        ("linux", true) => "aarch64-unknown-linux-gnu",
        ("macos", false) => "x86_64-apple-darwin",
        ("macos", true) => "aarch64-apple-darwin",
        _ => return None,
    })
}

/// Rust stable, installed by rustup-init (downloaded with its published SHA-256).
async fn rust_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    // rustup-init itself is the host's normal build; on Windows that's the MSVC one.
    let init_target = match (std::env::consts::OS, cfg!(target_arch = "aarch64")) {
        ("windows", false) => "x86_64-pc-windows-msvc",
        ("windows", true) => "aarch64-pc-windows-msvc",
        _ => rust_host().ok_or_else(|| unsupported("Rust"))?,
    };
    let channel = client
        .get("https://static.rust-lang.org/dist/channel-rust-stable.toml")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(http)?
        .text()
        .await
        .map_err(http)?;
    // [pkg.rust]
    // version = "1.99.1 (58cf8f9c3 2026-10-13)"
    let version = channel
        .split("[pkg.rust]")
        .nth(1)
        .and_then(|s| s.lines().find_map(|l| l.strip_prefix("version = \"")))
        .and_then(|v| v.split_whitespace().next())
        .ok_or_else(|| Error::msg("Couldn't read Rust's stable version"))?
        .to_string();
    let url = format!("https://static.rust-lang.org/rustup/dist/{init_target}/rustup-init{}", if cfg!(windows) { ".exe" } else { "" });
    let sha256 = published_hash(client, &format!("{url}.sha256")).await;
    Ok(vec![Release { runtime: Runtime::Rust, version, url, sha256, sha512: None, md5: None, sha3_256: None, archive: Archive::File, label: Some("Stable".into()) }])
}

/// RUSTUP_HOME and CARGO_HOME for a LampRabit Rust: inside its own folder.
pub fn rust_env(layout: &Layout, version: &str) -> Vec<(String, String)> {
    let home = layout.bin("rust", version);
    vec![
        ("RUSTUP_HOME".into(), home.join("rustup").to_string_lossy().into_owned()),
        ("CARGO_HOME".into(), home.join("cargo").to_string_lossy().into_owned()),
    ]
}

/// Runs the downloaded rustup-init for the given toolchain, without touching PATH or the
/// user's own ~/.cargo and ~/.rustup. Leaves nothing behind when it fails.
pub async fn setup_rust(layout: &Layout, version: &str) -> Result<()> {
    let home = layout.bin("rust", version);
    let result = async {
        let init = home.join(exe("rustup-init"));
        std::fs::rename(home.join("installed.txt"), &init)?;
        let mut args: Vec<String> = ["-y", "--no-modify-path", "--profile", "minimal", "--default-toolchain", version].iter().map(|s| s.to_string()).collect();
        if let Some(host) = rust_host() {
            args.extend(["--default-host".to_string(), host.to_string()]);
        }
        run_quiet_env(&init, &args, &home, &rust_env(layout, version)).await?;
        let _ = std::fs::remove_file(&init);
        std::fs::write(home.join("installed.txt"), format!("Rust {version}\n"))?;
        Ok::<_, Error>(())
    }
    .await;
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&home);
        return Err(Error::msg(format!("Installing Rust failed: {e}")));
    }
    Ok(())
}

async fn run_quiet_env(program: &Path, args: &[String], cwd: &Path, env: &[(String, String)]) -> Result<()> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args).current_dir(cwd).stdin(std::process::Stdio::null()).envs(env.iter().map(|(k, v)| (k, v)));
    crate::procgroup::prepare(&mut cmd);
    let out = cmd.output().await?;
    if out.status.success() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&out.stderr);
    let last = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
    Err(Error::msg(format!("{} failed: {last}", program.display())))
}

/// NVM's Node for n8n: its default version, which must meet n8n's minimum major.
pub fn n8n_node(minimum: u32) -> Result<PathBuf> {
    let (version, bin) = crate::node::for_project(None, Path::new("."))
        .map_err(|_| Error::msg("n8n runs on Node.js: install Node.js 24 or newer on the Services page (Languages → Node.js) first."))?;
    let major: u32 = version.trim_start_matches('v').split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0);
    if major < minimum {
        return Err(Error::msg(format!("n8n needs Node.js {minimum} or newer, and NVM's default is {version}. Install a newer Node.js on the Services page and make it the default.")));
    }
    Ok(bin)
}

/// Installs a downloaded n8n into its own folder with npm, on NVM's Node. Leaves nothing
/// behind when it fails.
pub async fn setup_n8n(layout: &Layout, version: &str) -> Result<()> {
    let home = layout.bin("n8n", version);
    let result = async {
        let bin = n8n_node(24)?;
        // The package tarball was saved as the marker; npm installs from the registry.
        let _ = std::fs::remove_file(home.join("installed.txt"));
        // From a clean folder: an interrupted earlier install leaves empty package folders
        // that npm would take as installed (n8n then fails on a missing module).
        let _ = std::fs::remove_dir_all(home.join("node_modules"));
        for file in ["package.json", "package-lock.json"] {
            let _ = std::fs::remove_file(home.join(file));
        }
        let npm = bin.join(if cfg!(windows) { "npm.cmd" } else { "npm" });
        let path = crate::apps::node_path(&bin, layout);
        let mut cmd = tokio::process::Command::new(&npm);
        cmd.args(["install", "--prefix"])
            .arg(&home)
            .arg(format!("n8n@{version}"))
            .args(["--omit=dev", "--no-audit", "--no-fund", "--loglevel=error"])
            .env("PATH", path)
            .current_dir(&home)
            .stdin(std::process::Stdio::null());
        crate::procgroup::prepare(&mut cmd);
        let out = cmd.output().await?;
        if !out.status.success() {
            let text = String::from_utf8_lossy(&out.stderr);
            let last = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
            return Err(Error::msg(format!("npm install failed: {last}")));
        }
        std::fs::write(home.join("installed.txt"), format!("n8n {version}\n"))?;
        Ok::<_, Error>(())
    }
    .await;
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&home);
        return Err(Error::msg(format!("Installing n8n failed: {e}")));
    }
    Ok(())
}

/// The ollama program of an Ollama installed outside LampRabit (its own installer or app),
/// which many people already have. LampRabit uses it instead of downloading another.
pub fn ollama_elsewhere() -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|d| vec![PathBuf::from(d).join("Programs").join("Ollama").join("ollama.exe")]).unwrap_or_default()
    } else if cfg!(target_os = "macos") {
        vec![PathBuf::from("/Applications/Ollama.app/Contents/Resources/ollama"), PathBuf::from("/usr/local/bin/ollama"), PathBuf::from("/opt/homebrew/bin/ollama")]
    } else {
        vec![PathBuf::from("/usr/local/bin/ollama"), PathBuf::from("/usr/bin/ollama")]
    };
    candidates.into_iter().find(|p| p.exists())
}

/// The executable of an installed Compass: the app bundle on macOS, otherwise the file
/// next to resources/ named after Compass.
pub fn compass_program(home: &Path) -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        return Some(home.join("MongoDB Compass.app")).filter(|p| p.exists());
    }
    std::fs::read_dir(home).ok()?.flatten().map(|e| e.path()).find(|p| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_lowercase();
        p.is_file() && name.contains("compass") && (if cfg!(windows) { name.ends_with(".exe") && !name.contains("update") } else { !name.contains('.') })
    })
}

/// ngrok publishes a "v3 stable" download per platform, without a version number in the
/// URL or a checksum, so this download relies on HTTPS.
fn ngrok_releases() -> Vec<Release> {
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "amd64" };
    let (os, ext, archive) = match std::env::consts::OS {
        "windows" => ("windows", "zip", Archive::Zip),
        "macos" => ("darwin", "zip", Archive::Zip),
        _ => ("linux", "tgz", Archive::TarGz),
    };
    vec![Release {
        runtime: Runtime::Ngrok,
        version: "3-stable".into(),
        url: format!("https://bin.equinox.io/c/bNyj1mQVY4c/ngrok-v3-stable-{os}-{arch}.{ext}"),
        sha256: None,
        sha512: None,
        md5: None,
        sha3_256: None,
        archive,
        label: Some("Stable channel".into()),
    }]
}

async fn cloudflared_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "amd64" };
    let (name, archive) = match std::env::consts::OS {
        "windows" => (format!("cloudflared-windows-{arch}.exe"), Archive::File),
        "macos" => (format!("cloudflared-darwin-{arch}.tgz"), Archive::TarGz),
        _ => (format!("cloudflared-linux-{arch}"), Archive::File),
    };
    github_latest(client, Runtime::Cloudflared, "cloudflare/cloudflared", archive, |n, _| n == name).await
}

async fn adminer_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    // The all-drivers, all-languages single file.
    github_latest(client, Runtime::Adminer, "vrana/adminer", Archive::File, |n, v| n == format!("adminer-{v}.php"))
        .await
}

/// MySQL's own download pages block scripts, so the current version of each release line
/// comes from the official Docker image's metadata (maintained with every release), and
/// the archives from MySQL's CDN, which publishes an MD5 next to each.
async fn mysql_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let (suffix, archive) = match std::env::consts::OS {
        "windows" => ("winx64.zip", Archive::Zip),
        // "minimal": stripped of debug symbols and tests, about a tenth of the full archive.
        "linux" if cfg!(target_arch = "x86_64") => ("linux-glibc2.28-x86_64-minimal.tar.xz", Archive::TarXz),
        "linux" => ("linux-glibc2.28-aarch64-minimal.tar.xz", Archive::TarXz),
        _ => return Err(unsupported("MySQL")),
    };
    let lines = get_json(client, "https://raw.githubusercontent.com/docker-library/mysql/master/versions.json").await?;
    let fetches = lines.as_object().into_iter().flatten().filter_map(|(line, info)| {
        let version = info["version"].as_str()?.to_string();
        let label = Some(if line == "innovation" { "Innovation".to_string() } else { "LTS".to_string() });
        Some(async move {
            let series = version.rsplit_once('.').map_or(version.as_str(), |(s, _)| s).to_string();
            let url = format!("https://cdn.mysql.com/Downloads/MySQL-{series}/mysql-{version}-{suffix}");
            let md5 = client
                .get(format!("{url}.md5"))
                .send()
                .await
                .and_then(|r| r.error_for_status())
                .ok()?
                .text()
                .await
                .ok()?
                .split_whitespace()
                .next()
                .map(str::to_string);
            md5.as_ref()?;
            Some(Release { runtime: Runtime::Mysql, version, url, sha256: None, sha512: None, md5, sha3_256: None, archive, label })
        })
    });
    let releases: Vec<Release> = futures_util::future::join_all(fetches.collect::<Vec<_>>()).await.into_iter().flatten().collect();
    if releases.is_empty() {
        return Err(Error::msg("Couldn't find MySQL downloads for this system"));
    }
    Ok(releases)
}

/// Redis has no official Windows build. The community redis-windows project compiles each
/// Redis release with MSYS2 (redis-server and redis-cli, with the msys DLLs they need next
/// to them) and GitHub publishes a SHA-256 for every archive. The newest patch release of
/// each of the last few lines is offered, like the other catalogs.
async fn redis_windows_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let list = get_json(client, "https://api.github.com/repos/redis-windows/redis-windows/releases?per_page=60").await?;
    let mut newest: Vec<Release> = Vec::new();
    for release in list.as_array().into_iter().flatten().filter(|r| r["prerelease"] == false && r["draft"] == false) {
        let version = release["tag_name"].as_str().unwrap_or_default().trim_start_matches('v').to_string();
        let name = format!("Redis-{version}-Windows-x64-msys2.zip");
        let Some(asset) = release["assets"].as_array().into_iter().flatten().find(|a| a["name"] == name.as_str()) else { continue };
        if newest.iter().any(|n| line(&n.version) == line(&version) && version_key(&n.version) >= version_key(&version)) {
            continue;
        }
        newest.retain(|n| line(&n.version) != line(&version));
        newest.push(Release {
            runtime: Runtime::Redis,
            url: asset["browser_download_url"].as_str().unwrap_or_default().to_string(),
            sha256: asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_string),
            sha512: None,
            md5: None,
            sha3_256: None,
            version,
            archive: Archive::Zip,
            label: Some("redis-windows (community build)".into()),
        });
    }
    if newest.is_empty() {
        return Err(Error::msg("Couldn't find Redis builds for Windows on github.com/redis-windows"));
    }
    newest.sort_by(|a, b| version_key(&b.version).cmp(&version_key(&a.version)));
    newest.truncate(4);
    Ok(newest)
}

/// phpRedisAdmin publishes source releases on GitHub (no separate archives or checksums,
/// so this relies on HTTPS). Its PHP dependency, Predis, is added after install.
async fn phpredisadmin_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let latest = get_json(client, "https://api.github.com/repos/erikdubbelboer/phpRedisAdmin/releases/latest").await?;
    let tag = latest["tag_name"].as_str().ok_or_else(|| Error::msg("unexpected phpRedisAdmin release data"))?;
    Ok(vec![Release {
        runtime: Runtime::Phpredisadmin,
        version: tag.trim_start_matches('v').to_string(),
        url: format!("https://github.com/erikdubbelboer/phpRedisAdmin/archive/refs/tags/{tag}.zip"),
        sha256: None,
        sha512: None,
        md5: None,
        sha3_256: None,
        archive: Archive::Zip,
        label: None,
    }])
}

/// phpRedisAdmin talks to Redis through Predis, normally installed by Composer. LampRabit
/// adds the exact version its composer.json asks for, with a two-line autoloader, so
/// neither Composer nor PHP is needed to install it.
pub async fn add_predis(client: &reqwest::Client, layout: &Layout, version: &str) -> Result<()> {
    let home = layout.bin("phpredisadmin", version);
    let target = home.join("vendor").join("predis").join("predis");
    if target.join("autoload.php").exists() {
        return Ok(());
    }
    let composer: Value = serde_json::from_str(&std::fs::read_to_string(home.join("composer.json"))?)
        .map_err(|e| Error::msg(format!("phpRedisAdmin's composer.json: {e}")))?;
    let predis = composer["require"]["predis/predis"].as_str().unwrap_or("v2.4.1").trim_start_matches(['^', '~', '=']);
    let tag = if predis.starts_with('v') { predis.to_string() } else { format!("v{predis}") };
    let bytes = client
        .get(format!("https://github.com/predis/predis/archive/refs/tags/{tag}.zip"))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(http)?
        .bytes()
        .await
        .map_err(http)?;
    let scratch = layout.tmp().join(format!("predis-{tag}.extract"));
    let _ = std::fs::remove_dir_all(&scratch);
    zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .and_then(|mut z| z.extract(&scratch))
        .map_err(|e| Error::msg(format!("could not extract Predis: {e}")))?;
    let root = find_root(&scratch, Path::new("autoload.php"), 2).ok_or_else(|| Error::msg("the Predis archive has no autoload.php"))?;
    std::fs::create_dir_all(target.parent().unwrap())?;
    std::fs::rename(root, &target)?;
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::write(home.join("vendor").join("autoload.php"), "<?php\n// Written by LampRabit in place of Composer's autoloader.\nrequire __DIR__ . '/predis/predis/autoload.php';\n")?;
    Ok(())
}

/// getcomposer.org lists the current release and the 2.2 LTS line (the last one that
/// runs on PHP below 7.2.5), each with a published checksum.
async fn composer_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    const SITE: &str = "https://getcomposer.org";
    let index = get_json(client, &format!("{SITE}/versions")).await?;
    let mut seen = Vec::new();
    let fetches = index["stable"].as_array().into_iter().flatten().filter_map(|r| {
        let version = r["version"].as_str()?.to_string();
        let path = r["path"].as_str()?.to_string();
        if seen.contains(&version) {
            return None;
        }
        seen.push(version.clone());
        // Composer's LTS line (2.2) is the one kept for PHP 5.3–7.1, not the one to pick
        // for a new setup; the label says so, and one-click installs don't prefer it.
        let label = r["lts"].as_bool().unwrap_or(false).then(|| "LTS for PHP 5.3–7.1".to_string());
        Some(async move {
            let url = format!("{SITE}{path}");
            let sha256 = client
                .get(format!("{url}.sha256sum"))
                .send()
                .await
                .and_then(|r| r.error_for_status())
                .map_err(http)?
                .text()
                .await
                .map_err(http)?
                .split_whitespace()
                .next()
                .map(str::to_string);
            // Never install Composer without a checksum: it runs with your project's rights.
            if sha256.is_none() {
                return Err(Error::msg(format!("getcomposer.org has no checksum for Composer {version}")));
            }
            Ok(Release { runtime: Runtime::Composer, version, url, sha256, sha512: None, md5: None, sha3_256: None, archive: Archive::File, label })
        })
    });
    futures_util::future::join_all(fetches.collect::<Vec<_>>()).await.into_iter().collect()
}

/// Composer used to live directly in `bin/composer/composer.phar`; moves it into a
/// version folder like every other runtime. The version is read from the phar itself.
pub fn migrate_flat_composer(layout: &Layout) {
    let dir = layout.root().join("bin").join("composer");
    let flat = dir.join("composer.phar");
    let Ok(bytes) = std::fs::read(&flat) else { return };
    let text = String::from_utf8_lossy(&bytes);
    let version = text
        .split("const VERSION = '")
        .nth(1)
        .and_then(|s| s.split('\'').next())
        .filter(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-'));
    if let Some(v) = version {
        let dest = dir.join(v);
        if std::fs::create_dir_all(&dest).is_ok() && !dest.join("composer.phar").exists() {
            let _ = std::fs::rename(&flat, dest.join("composer.phar"));
        }
    }
    // Unreadable or a duplicate: a fresh copy is installed the next time it's needed.
    let _ = std::fs::remove_file(&flat);
}

/// LampRabit 0.1 ran Microsoft Garnet as Redis, with a private .NET runtime and (on
/// Windows) a separate redis-cli. Real Redis replaced it everywhere, so those folders are
/// deleted; Garnet's versions have no redis-server and already count as not installed.
/// Returns whether Garnet was found, so the user can be told to install Redis again.
pub fn remove_garnet(layout: &Layout) -> bool {
    let bin = layout.root().join("bin");
    let garnet: Vec<PathBuf> = std::fs::read_dir(bin.join("redis"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join(exe("GarnetServer")).exists() && !p.join(Runtime::Redis.marker()).exists())
        .collect();
    for dir in &garnet {
        let _ = std::fs::remove_dir_all(dir);
    }
    for old in ["dotnet", "rediscli"] {
        let _ = std::fs::remove_dir_all(bin.join(old));
    }
    !garnet.is_empty()
}

async fn phpmyadmin_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let info = get_json(client, "https://www.phpmyadmin.net/home_page/version.json").await?;
    let version = info["version"].as_str().ok_or_else(|| Error::msg("unexpected phpMyAdmin version data"))?;
    let php = info["releases"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|r| r["version"] == version)
        .and_then(|r| r["php_versions"].as_str())
        // ">=7.2,<8.4" -> "PHP <8.4": the upper bound is what users run into.
        .map(|p| format!("PHP {}", p.rsplit(',').next().unwrap_or(p).trim()));
    let url = format!("https://files.phpmyadmin.net/phpMyAdmin/{version}/phpMyAdmin-{version}-all-languages.zip");
    let sha256 = match client.get(format!("{url}.sha256")).send().await {
        Ok(r) if r.status().is_success() => {
            r.text().await.ok().and_then(|t| t.split_whitespace().next().map(str::to_string))
        }
        _ => None,
    };
    Ok(vec![Release {
        runtime: Runtime::Phpmyadmin,
        version: version.to_string(),
        url,
        sha256,
        sha512: None,
        md5: None,
        sha3_256: None,
        archive: Archive::Zip,
        label: php,
    }])
}

pub fn installed(layout: &Layout, runtime: Runtime) -> Vec<String> {
    let dir = layout.root().join("bin").join(runtime.id());
    let mut versions: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join(runtime.marker()).exists())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .collect();
    versions.sort_by(|a, b| version_key(b).cmp(&version_key(a)));
    versions
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    pub runtime: Runtime,
    pub version: String,
    pub stage: &'static str,
    pub downloaded: u64,
    pub total: Option<u64>,
}

/// Runtimes whose install continues after the download (npm, pip, rustup, Predis): the
/// installer reports "setup" rather than "done", and the app reports done when it ends.
pub fn has_setup(runtime: Runtime) -> bool {
    matches!(runtime, Runtime::Rust | Runtime::N8n | Runtime::Pgadmin | Runtime::Phpredisadmin)
}

pub async fn install(app: &AppHandle, client: &reqwest::Client, layout: &Layout, release: &Release) -> Result<()> {
    let runtime = release.runtime;
    let dest = layout.bin(runtime.id(), &release.version);
    if dest.join(runtime.marker()).exists() {
        return Ok(());
    }
    let progress = |stage, downloaded, total| {
        let _ = app.emit(
            "install-progress",
            InstallProgress { runtime, version: release.version.clone(), stage, downloaded, total },
        );
    };

    std::fs::create_dir_all(layout.tmp())?;
    let name = format!("{}-{}", runtime.id(), release.version);
    let archive_path = layout.tmp().join(format!("{name}.download"));
    let extract_dir = layout.tmp().join(format!("{name}.extract"));

    let result = async {
        let response = client.get(&release.url).send().await.and_then(|r| r.error_for_status()).map_err(http)?;
        let total = response.content_length();
        let mut file = tokio::fs::File::create(&archive_path).await?;
        let mut hasher = Sha256::new();
        let mut hasher512 = sha2::Sha512::new();
        let mut hasher_md5 = md5::Md5::new();
        let mut hasher_sha3 = sha3::Sha3_256::new();
        let mut downloaded = 0u64;
        let mut last_emit = Instant::now();
        let mut stream = response.bytes_stream();
        progress("downloading", 0, total);
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(http)?;
            hasher.update(&chunk);
            hasher512.update(&chunk);
            hasher_md5.update(&chunk);
            hasher_sha3.update(&chunk);
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            if last_emit.elapsed() > Duration::from_millis(150) {
                progress("downloading", downloaded, total);
                last_emit = Instant::now();
            }
        }
        file.flush().await?;
        drop(file);

        progress("verifying", downloaded, total);
        let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let checks = [
            (&release.sha256, hex(&hasher.finalize())),
            (&release.sha512, hex(&hasher512.finalize())),
            (&release.md5, hex(&hasher_md5.finalize())),
            (&release.sha3_256, hex(&hasher_sha3.finalize())),
        ];
        for (expected, actual) in checks {
            if let Some(expected) = expected {
                if !actual.eq_ignore_ascii_case(expected) {
                    return Err(Error::msg(format!("checksum mismatch for {name}: download is corrupt")));
                }
            }
        }

        progress("extracting", downloaded, total);
        let (archive_path, extract_dir, dest, marker, kind) =
            (archive_path.clone(), extract_dir.clone(), dest.clone(), runtime.marker(), release.archive);
        tokio::task::spawn_blocking(move || extract_into(&archive_path, kind, &extract_dir, &dest, &marker))
            .await
            .map_err(|e| Error::msg(e.to_string()))??;
        progress(if has_setup(runtime) { "setup" } else { "done" }, downloaded, total);
        Ok(())
    }
    .await;

    let _ = std::fs::remove_file(&archive_path);
    let _ = std::fs::remove_dir_all(&extract_dir);
    if result.is_err() {
        progress("error", 0, None);
    }
    result
}

fn extract_into(archive: &Path, kind: Archive, scratch: &Path, dest: &Path, marker: &Path) -> Result<()> {
    if scratch.exists() {
        std::fs::remove_dir_all(scratch)?;
    }
    if let Archive::File = kind {
        let target = dest.join(marker);
        std::fs::create_dir_all(target.parent().unwrap_or(dest))?;
        std::fs::copy(archive, &target)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))?;
        }
        return Ok(());
    }
    let file = std::fs::File::open(archive)?;
    match kind {
        Archive::File => unreachable!(),
        Archive::Zip => zip::ZipArchive::new(file)
            .and_then(|mut z| z.extract(scratch))
            .map_err(|e| Error::msg(format!("could not extract archive: {e}")))?,
        Archive::TarGz => tar::Archive::new(flate2::read::GzDecoder::new(file)).unpack(scratch)?,
        Archive::TarZst => {
            let decoder = ruzstd::decoding::StreamingDecoder::new(std::io::BufReader::new(file))
                .map_err(|e| Error::msg(format!("could not decompress archive: {e}")))?;
            tar::Archive::new(decoder).unpack(scratch)?
        }
        Archive::TarXz => {
            // Through a file rather than memory: unpacked, MySQL's archive is several hundred MB.
            let tar_path = scratch.with_extension("tar");
            let result = (|| {
                let mut out = std::io::BufWriter::new(std::fs::File::create(&tar_path)?);
                lzma_rs::xz_decompress(&mut std::io::BufReader::new(file), &mut out)
                    .map_err(|e| Error::msg(format!("could not decompress archive: {e}")))?;
                std::io::Write::flush(&mut out)?;
                drop(out);
                tar::Archive::new(std::fs::File::open(&tar_path)?).unpack(scratch)?;
                Ok::<_, Error>(())
            })();
            let _ = std::fs::remove_file(&tar_path);
            result?
        }
    }

    // Archives wrap their content differently (Apache24/, mariadb-x.y-winx64/, or nothing);
    // the runtime root is whichever directory contains the marker file. macOS JDKs are the
    // deepest: jdk-21.0.5+11/Contents/Home/bin/java.
    let root = find_root(scratch, marker, 3)
        .ok_or_else(|| Error::msg(format!("archive does not contain {}", marker.display())))?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if dest.exists() {
        std::fs::remove_dir_all(dest)?;
    }
    std::fs::rename(root, dest)?;
    Ok(())
}

fn find_root(dir: &Path, marker: &Path, depth: u32) -> Option<PathBuf> {
    if dir.join(marker).exists() {
        return Some(dir.to_path_buf());
    }
    if depth == 0 {
        return None;
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .find_map(|e| find_root(&e.path(), marker, depth - 1))
}

pub fn remove(layout: &Layout, runtime: Runtime, version: &str) -> Result<()> {
    let dir = layout.bin(runtime.id(), version);
    if !dir.starts_with(layout.root().join("bin")) || version.contains("..") {
        return Err(Error::msg("invalid version"));
    }
    // Rename first: Windows refuses while any file inside is in use, so a version is
    // either removed completely or left untouched, never half-deleted.
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let trash = dir.with_file_name(format!(".removing-{version}-{stamp}"));
    std::fs::rename(&dir, &trash).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied || e.raw_os_error() == Some(32) {
            Error::msg(format!(
                "{} {version} is still in use by a running program. Stop the services using it (or close any terminal running it) and try again.",
                runtime.id()
            ))
        } else {
            Error::from(e)
        }
    })?;
    if std::fs::remove_dir_all(&trash).is_err() {
        // Already out of the way and invisible to LampRabit; cleaned up on a later start.
        eprintln!("could not finish deleting {}", trash.display());
    }
    Ok(())
}

/// Deletes leftovers of removals that couldn't finish (see `remove`).
pub fn clean_removed(layout: &Layout) {
    let Ok(runtimes) = std::fs::read_dir(layout.root().join("bin")) else { return };
    for runtime in runtimes.flatten() {
        let Ok(entries) = std::fs::read_dir(runtime.path()) else { continue };
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(".removing-") {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNotice {
    pub runtime: Runtime,
    pub installed: String,
    pub release: Release,
}

/// Newer patch releases for installed versions: same "major.minor" line (for example
/// PHP 8.4.27 → 8.4.30), never a jump to a new minor that could change behaviour.
pub async fn updates(client: &reqwest::Client, layout: &Layout) -> Vec<UpdateNotice> {
    let checks = [Runtime::Apache, Runtime::Nginx, Runtime::Php, Runtime::Mariadb, Runtime::Mailpit, Runtime::Adminer, Runtime::Phpmyadmin, Runtime::Redis, Runtime::Composer, Runtime::Mysql, Runtime::Phpredisadmin, Runtime::Memcached, Runtime::Caddy, Runtime::Java, Runtime::Tomcat, Runtime::Frankenphp, Runtime::Postgres, Runtime::Python, Runtime::Phppgadmin, Runtime::Pgadmin, Runtime::Mongodb, Runtime::Mongosh, Runtime::Mongotools, Runtime::Compass, Runtime::Sqlite, Runtime::Clickhouse, Runtime::Neo4j, Runtime::Meilisearch, Runtime::Typesense, Runtime::Rustfs, Runtime::Ollama, Runtime::Minio, Runtime::Elasticsearch, Runtime::Opensearch, Runtime::N8n, Runtime::Rabbitmq, Runtime::Erlang, Runtime::Go, Runtime::Maven, Runtime::Gradle]
        .into_iter()
        .map(|runtime| async move {
            let installed = installed(layout, runtime);
            if installed.is_empty() {
                return Vec::new();
            }
            let Ok(catalog) = available(client, runtime, false).await else { return Vec::new() };
            let mut out = Vec::new();
            for have in &installed {
                if let Some(newer) = catalog.iter().find(|r| {
                    line(&r.version) == line(have) && version_key(&r.version) > version_key(have) && !installed.contains(&r.version)
                }) {
                    out.push(UpdateNotice { runtime, installed: have.clone(), release: newer.clone() });
                }
            }
            out
        });
    futures_util::future::join_all(checks).await.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mongodb_build_follows_the_linux_distribution() {
        let os = |id: &str, version: &str, extra: &str| format!("ID={id}\nVERSION_ID=\"{version}\"\n{extra}");
        assert_eq!(mongodb_target_for(&os("ubuntu", "24.04", "")).unwrap(), "ubuntu2404");
        assert_eq!(mongodb_target_for(&os("ubuntu", "25.10", "")).unwrap(), "ubuntu2404", "between releases: the older build");
        assert_eq!(mongodb_target_for(&os("debian", "13", "")).unwrap(), "debian13");
        assert_eq!(mongodb_target_for(&os("rocky", "9.4", "")).unwrap(), "rhel93");
        assert_eq!(mongodb_target_for(&os("linuxmint", "22", "ID_LIKE=\"ubuntu debian\"\nUBUNTU_CODENAME=jammy")).unwrap(), "ubuntu2204");
        assert!(mongodb_target_for(&os("arch", "", "PRETTY_NAME=\"Arch Linux\"")).unwrap_err().to_string().contains("Arch Linux"));
    }
}
