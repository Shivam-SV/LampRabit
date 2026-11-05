//! Things done to a whole project from its page: whether its database exists (and creating
//! or dropping it), fixing a .env value, checking new project names, and whether ports
//! answer. Renaming and deleting, which need the app's state, are in lib.rs.

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use serde::Serialize;

use crate::{
    config::{Database, Settings},
    configs,
    error::{Error, Result},
    layout::{Layout, exe},
    procgroup,
    services::ServiceKind,
};

/// The database server a project's config points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Mariadb,
    Mysql,
    Postgres,
    Sqlite,
}

impl Engine {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mariadb => "MariaDB",
            Self::Mysql => "MySQL",
            Self::Postgres => "PostgreSQL",
            Self::Sqlite => "SQLite",
        }
    }

    /// The service that has to run for the database to be reached; none for SQLite.
    pub fn service(self) -> Option<ServiceKind> {
        match self {
            Self::Mariadb => Some(ServiceKind::Mariadb),
            Self::Mysql => Some(ServiceKind::Mysql),
            Self::Postgres => Some(ServiceKind::Postgres),
            Self::Sqlite => None,
        }
    }

    fn installed(self, settings: &Settings) -> bool {
        match self {
            Self::Mariadb => settings.versions.mariadb.is_some(),
            Self::Mysql => settings.versions.mysql.is_some(),
            Self::Postgres => settings.versions.get("postgres").is_some(),
            Self::Sqlite => true,
        }
    }
}

/// Whether a project's database can be used: its server, and whether the database exists.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbStatus {
    pub engine: Engine,
    pub label: &'static str,
    pub database: String,
    pub installed: bool,
    pub running: bool,
    /// None when it can't be told (PostgreSQL while it's stopped).
    pub exists: Option<bool>,
}

fn env_text(dir: &Path) -> String {
    [".env.local", ".env"].iter().filter_map(|f| std::fs::read_to_string(dir.join(f)).ok()).collect::<Vec<_>>().join("\n")
}

fn env_value(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix(&format!("{key}=")))
        .map(|v| v.trim().trim_matches(['"', '\'']).to_string())
        .filter(|v| !v.is_empty())
}

/// The port in "mysql://user:pass@127.0.0.1:3307/shop" or "localhost:3307".
fn port_in(address: &str) -> Option<u16> {
    let host = address.rsplit('@').next()?.split(['/', '?']).next()?;
    host.rsplit_once(':')?.1.parse().ok()
}

/// MariaDB or MySQL, by the port the project connects to; the default one otherwise.
fn mysql_family(settings: &Settings, port: Option<u16>, hint: Option<Engine>) -> Option<Engine> {
    let by_port = match port {
        Some(p) if p == settings.ports.mysql_server => Some(Engine::Mysql),
        Some(p) if p == settings.ports.mysql => Some(Engine::Mariadb),
        _ => None,
    };
    by_port.or(hint).or_else(|| {
        settings.database().map(|d| match d {
            Database::Mariadb => Engine::Mariadb,
            Database::Mysql => Engine::Mysql,
        })
    })
}

/// Which server a project's database lives on, from its .env (Laravel's DB_CONNECTION,
/// Symfony's DATABASE_URL) or wp-config.php's DB_HOST.
pub fn engine(settings: &Settings, dir: &Path) -> Option<Engine> {
    let env = env_text(dir);
    if let Some(connection) = env_value(&env, "DB_CONNECTION") {
        let port = env_value(&env, "DB_PORT").and_then(|p| p.parse().ok());
        return match connection.as_str() {
            "sqlite" => Some(Engine::Sqlite),
            "pgsql" | "postgres" | "postgresql" => Some(Engine::Postgres),
            "mariadb" => mysql_family(settings, port, Some(Engine::Mariadb)),
            _ => mysql_family(settings, port, None),
        };
    }
    if let Some(url) = env_value(&env, "DATABASE_URL") {
        let scheme = url.split("://").next().unwrap_or_default();
        return match scheme {
            s if s.starts_with("postgres") => Some(Engine::Postgres),
            s if s.starts_with("sqlite") => Some(Engine::Sqlite),
            "mariadb" => mysql_family(settings, port_in(&url), Some(Engine::Mariadb)),
            _ => mysql_family(settings, port_in(&url), None),
        };
    }
    let wp = std::fs::read_to_string(dir.join("wp-config.php")).unwrap_or_default();
    let host = wp.find("'DB_HOST'").and_then(|at| {
        let rest = &wp[at + 9..];
        let start = rest.find(['\'', '"'])? + 1;
        let end = rest[start..].find(['\'', '"'])?;
        Some(rest[start..start + end].to_string())
    });
    mysql_family(settings, host.as_deref().and_then(port_in), None)
}

/// A database name MariaDB and MySQL keep as a folder: other characters than letters,
/// digits and _ are written as @00xx (shop-test is shop@002dtest).
fn folder_name(database: &str) -> String {
    database.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c.to_string() } else { format!("@{:04x}", c as u32) }).collect()
}

/// Names that are safe to quote into SQL: letters, digits, _, - and $.
fn check_name(database: &str) -> Result<()> {
    if database.is_empty() || database.len() > 64 || !database.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '$')) {
        return Err(Error::msg(format!("\"{database}\" can't be created from here: use letters, digits, _ and - in the database name.")));
    }
    Ok(())
}

/// The SQLite file a project names (relative to it), or Laravel's default.
fn sqlite_file(dir: &Path, database: &str) -> PathBuf {
    let path = if database.is_empty() { "database/database.sqlite" } else { database };
    let path = Path::new(path);
    if path.is_absolute() { path.to_path_buf() } else { dir.join(path) }
}

fn psql(settings: &Settings, layout: &Layout) -> Result<(PathBuf, PathBuf, Vec<String>)> {
    let version = settings.versions.get("postgres").ok_or_else(|| Error::msg("Install PostgreSQL first (Services)."))?;
    let home = layout.bin("postgres", version);
    let args = ["-h", "127.0.0.1", "-p", &settings.ports.postgres.to_string(), "-U", "postgres", "-d", "postgres", "-v", "ON_ERROR_STOP=1", "-tAc"].map(str::to_string).to_vec();
    Ok((home.join("bin").join(exe("psql")), home, args))
}

/// Runs a database client and returns what it printed.
async fn run(program: &Path, args: &[String], cwd: &Path) -> Result<String> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args).current_dir(cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    procgroup::prepare(&mut cmd);
    let child = cmd.spawn().map_err(|e| Error::msg(format!("Couldn't run {}: {e}", program.display())))?;
    let out = tokio::time::timeout(Duration::from_secs(30), child.wait_with_output())
        .await
        .map_err(|_| Error::msg("The database server didn't answer within 30 seconds."))??;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(Error::msg(if err.is_empty() { "The database command failed.".to_string() } else { err }));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Runs SQL on a MariaDB or MySQL server through its client and LampRabit's client.cnf.
async fn mysql_sql(settings: &Settings, layout: &Layout, db: Database, sql: String) -> Result<String> {
    let home = configs::database_home(settings, layout, db)?;
    let client = configs::database_client(settings, layout, db)?;
    let args = vec![format!("--defaults-file={}", configs::write_client_config(settings, layout, db)?.display()), "-N".into(), "-e".into(), sql];
    run(&client, &args, &home).await
}

fn family(engine: Engine) -> Option<Database> {
    match engine {
        Engine::Mariadb => Some(Database::Mariadb),
        Engine::Mysql => Some(Database::Mysql),
        _ => None,
    }
}

/// Whether a project's database exists. MariaDB and MySQL keep a folder per database,
/// so that works while they're stopped; PostgreSQL has to be asked.
pub async fn status(settings: &Settings, layout: &Layout, dir: &Path, database: &str, running: bool) -> Option<DbStatus> {
    let engine = engine(settings, dir)?;
    let installed = engine.installed(settings);
    let exists = match engine {
        Engine::Mariadb if installed => Some(layout.data("mariadb").join(folder_name(database)).is_dir()),
        Engine::Mysql if installed => Some(configs::mysql_data(settings, layout).join(folder_name(database)).is_dir()),
        Engine::Postgres if installed && running => {
            let (client, home, mut args) = psql(settings, layout).ok()?;
            args.push(format!("SELECT 1 FROM pg_database WHERE datname = '{}'", database.replace('\'', "''")));
            run(&client, &args, &home).await.ok().map(|out| out == "1")
        }
        Engine::Sqlite => Some(sqlite_file(dir, database).is_file()),
        _ => None,
    };
    Some(DbStatus { engine, label: engine.label(), database: database.to_string(), installed, running: running || engine == Engine::Sqlite, exists })
}

/// Creates the database a project names, on its server (which must be running).
pub async fn create(settings: &Settings, layout: &Layout, dir: &Path, engine: Engine, database: &str) -> Result<()> {
    if engine == Engine::Sqlite {
        let file = sqlite_file(dir, database);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::OpenOptions::new().create(true).append(true).open(&file)?;
        return Ok(());
    }
    check_name(database)?;
    if let Some(db) = family(engine) {
        mysql_sql(settings, layout, db, format!("CREATE DATABASE IF NOT EXISTS `{database}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci")).await?;
    } else {
        let (client, home, mut args) = psql(settings, layout)?;
        // template0 so the encoding can be set whatever template1 has.
        args.push(format!("CREATE DATABASE \"{database}\" ENCODING 'UTF8' TEMPLATE template0"));
        run(&client, &args, &home).await?;
    }
    Ok(())
}

/// Drops a project's database (when the project is deleted with it).
pub async fn drop(settings: &Settings, layout: &Layout, engine: Engine, database: &str) -> Result<()> {
    if engine == Engine::Sqlite {
        // The file is inside the project, which goes to the Recycle Bin with it.
        return Ok(());
    }
    check_name(database)?;
    if let Some(db) = family(engine) {
        mysql_sql(settings, layout, db, format!("DROP DATABASE IF EXISTS `{database}`")).await?;
    } else {
        let (client, home, mut args) = psql(settings, layout)?;
        args.push(format!("DROP DATABASE IF EXISTS \"{database}\" WITH (FORCE)"));
        run(&client, &args, &home).await?;
    }
    Ok(())
}

/// .env keys the page may change (the ones shown on its Overview).
pub fn set_env_value(dir: &Path, key: &str, value: &str) -> Result<()> {
    if !crate::facts::ENV_KEYS.contains(&key) {
        return Err(Error::msg(format!("{key} can't be changed from here; edit the .env file instead.")));
    }
    if value.contains(['\n', '\r']) {
        return Err(Error::msg("A .env value has to fit on one line."));
    }
    let file = [".env.local", ".env"].iter().map(|f| dir.join(f)).find(|p| p.is_file()).ok_or_else(|| Error::msg("The project has no .env file to change."))?;
    crate::projects::set_env(&file, &[(key.to_string(), value.to_string())])
}

/// Why a folder name can't be a project's new name, or Ok.
pub fn check_project_name(layout: &Layout, name: &str) -> Result<()> {
    let bad = name.is_empty()
        || name.len() > 64
        || name.starts_with('.')
        || name.ends_with(['.', ' '])
        || name.chars().any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control());
    if bad {
        return Err(Error::msg(format!("\"{name}\" can't be a folder name: use letters, digits, dashes and underscores.")));
    }
    if crate::sites::label(name).is_none_or(|l| l.is_empty()) {
        return Err(Error::msg(format!("\"{name}\" doesn't give a usable domain: use letters or digits in it.")));
    }
    // Windows folders ignore case, so "Shop" and "shop" are the same folder there.
    let taken = std::fs::read_dir(layout.www()).map(|d| d.flatten().any(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))).unwrap_or(false);
    if taken {
        return Err(Error::msg(format!("There's already a folder called {name} in www.")));
    }
    Ok(())
}

/// The ports among `ports` something answers on, on 127.0.0.1 or ::1 (dev servers that
/// listen on "localhost" may only take IPv6).
pub fn listening(ports: &[u16]) -> Vec<u16> {
    let timeout = Duration::from_millis(250);
    ports
        .iter()
        .copied()
        .filter(|&port| {
            ["127.0.0.1", "[::1]"].iter().any(|host| {
                format!("{host}:{port}").parse().is_ok_and(|addr| std::net::TcpStream::connect_timeout(&addr, timeout).is_ok())
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_folder_names() {
        assert_eq!(folder_name("shop"), "shop");
        assert_eq!(folder_name("shop_db"), "shop_db");
        assert_eq!(folder_name("shop-test"), "shop@002dtest");
    }

    #[test]
    fn ports_in_addresses() {
        assert_eq!(port_in("mysql://app:secret@127.0.0.1:3307/shop?serverVersion=8"), Some(3307));
        assert_eq!(port_in("localhost:3306"), Some(3306));
        assert_eq!(port_in("localhost"), None);
    }

    #[test]
    fn refuses_unsafe_names() {
        assert!(check_name("shop_db").is_ok());
        assert!(check_name("shop`; DROP").is_err());
        assert!(check_name("").is_err());
    }
}
