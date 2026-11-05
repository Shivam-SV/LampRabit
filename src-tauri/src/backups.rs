//! Database backups: one file per dump in `<root>/backups/<engine>/`, made and restored with
//! each server's own tools (mariadb-dump/mysqldump, pg_dump/pg_restore). SQLite has no
//! server: its databases are files in the projects, copied with sqlite3's `.backup` when
//! the shell is installed. Backups are the user's data, so uninstall keeps the folder like
//! www/ and data/.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::Serialize;

use crate::{
    config::{Database, Settings},
    configs,
    error::{Error, Result},
    layout::{Layout, exe},
    templates::DbEngine,
};

/// System databases that are never listed for backup.
const MYSQL_SYSTEM: &[&str] = &["mysql", "information_schema", "performance_schema", "sys"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub engine: DbEngine,
    pub database: String,
    pub path: PathBuf,
    pub size: u64,
    /// Unix time in milliseconds.
    pub created: u64,
}

pub fn folder(layout: &Layout) -> PathBuf {
    layout.root().join("backups")
}

fn mysql_family(engine: DbEngine) -> Option<Database> {
    match engine {
        DbEngine::Mariadb => Some(Database::Mariadb),
        DbEngine::Mysql => Some(Database::Mysql),
        _ => None,
    }
}

/// mongodump/mongorestore from the MongoDB Database Tools, pointed at LampRabit's MongoDB.
fn mongo_tool(settings: &Settings, layout: &Layout, name: &str) -> Result<(PathBuf, Vec<String>)> {
    let version = settings
        .versions
        .get("mongotools")
        .ok_or_else(|| Error::msg("MongoDB backups use the MongoDB Database Tools: install them on the Services page (Databases → MongoDB Database Tools)."))?;
    let program = layout.bin("mongotools", version).join("bin").join(exe(name));
    Ok((program, vec![format!("--uri=mongodb://127.0.0.1:{}", settings.ports.mongodb)]))
}

/// MongoDB's system databases, never listed for backup.
const MONGO_SYSTEM: &[&str] = &["admin", "config", "local"];

/// File types taken for SQLite databases in projects (Laravel's database.sqlite,
/// Symfony's var/data.db…).
const SQLITE_EXTENSIONS: &[&str] = &["sqlite", "sqlite3", "db"];
/// Folders that hold dependencies or caches, not the project's databases.
const SQLITE_SKIP: &[&str] = &["node_modules", "vendor", "storage/framework", "var/cache"];

/// SQLite databases in the projects, as "<project>/<path>" with forward slashes.
fn sqlite_databases(layout: &Layout) -> Vec<String> {
    fn walk(dir: &Path, rel: &str, depth: u8, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            let path = entry.path();
            if path.is_dir() {
                // Relative to the project, to match "storage/framework".
                let inner = rel.split_once('/').map_or("", |(_, r)| r);
                if depth > 0 && !name.starts_with('.') && !SQLITE_SKIP.contains(&inner) && !SQLITE_SKIP.contains(&name.as_str()) {
                    walk(&path, &rel, depth - 1, out);
                }
            } else if is_sqlite(&path) {
                out.push(rel);
            }
        }
    }
    let mut out = Vec::new();
    // www/<project>/database/database.sqlite is three levels down; a little more for monorepos.
    walk(&layout.www(), "", 4, &mut out);
    out.sort();
    out
}

/// An SQLite file by its header. Laravel creates database.sqlite empty, which SQLite
/// treats as an empty database; an empty .db could be anything.
fn is_sqlite(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    if !SQLITE_EXTENSIONS.contains(&ext.as_str()) {
        return false;
    }
    let mut head = [0u8; 16];
    match std::fs::File::open(path).and_then(|mut f| std::io::Read::read(&mut f, &mut head)) {
        Ok(0) => ext != "db",
        Ok(16) => &head == b"SQLite format 3\0",
        _ => false,
    }
}

/// The file behind "<project>/<path>", which has to stay inside www/.
fn sqlite_file(layout: &Layout, database: &str) -> Result<PathBuf> {
    let rel = Path::new(database);
    let plain = rel.components().all(|c| matches!(c, std::path::Component::Normal(_)));
    let ext = rel.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    // "~" stands for "/" in backup file names.
    if database.is_empty() || !plain || !SQLITE_EXTENSIONS.contains(&ext.as_str()) || database.contains('~') {
        return Err(Error::msg("Pick an SQLite file inside a project (www/<project>/…, ending in .sqlite, .sqlite3 or .db)."));
    }
    Ok(layout.www().join(rel))
}

/// The sqlite3 shell, when installed: its `.backup` copies a database safely while an
/// app writes to it, including what's still in the WAL file.
fn sqlite_shell(settings: &Settings, layout: &Layout) -> Option<PathBuf> {
    let version = settings.versions.get("sqlite")?;
    Some(layout.bin("sqlite", version).join(exe("sqlite3"))).filter(|p| p.exists())
}

/// `<file>-wal` and the like, next to a database.
fn sqlite_side(file: &Path, suffix: &str) -> PathBuf {
    let mut side = file.as_os_str().to_owned();
    side.push(suffix);
    PathBuf::from(side)
}

/// Removes a database's WAL and shared-memory files, which would otherwise be replayed
/// onto the restored file and corrupt it.
fn remove_sqlite_journal(file: &Path) -> Result<()> {
    for suffix in ["-wal", "-shm", "-journal"] {
        match std::fs::remove_file(sqlite_side(file, suffix)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(Error::msg(format!("Couldn't replace {}: {e}. Stop the app that uses it, then try again.", file.display())));
            }
            _ => {}
        }
    }
    Ok(())
}

/// `pg_dump`, `psql`… from the active PostgreSQL, with the arguments that reach
/// LampRabit's server as its superuser.
fn postgres_tool(settings: &Settings, layout: &Layout, name: &str) -> Result<(PathBuf, Vec<String>)> {
    let version = settings.versions.get("postgres").ok_or_else(|| Error::msg("PostgreSQL is not installed"))?;
    let program = layout.bin("postgres", version).join("bin").join(exe(name));
    let args = vec!["-h".into(), "127.0.0.1".into(), "-p".into(), settings.ports.postgres.to_string(), "-U".into(), "postgres".into()];
    Ok((program, args))
}

/// A MariaDB/MySQL tool (`mariadb-dump`, `mysql`…) with LampRabit's client config, which
/// has the host, port and user.
fn mysql_tool(settings: &Settings, layout: &Layout, db: Database, mariadb: &str, mysql: &str) -> Result<(PathBuf, Vec<String>)> {
    let name = if db == Database::Mariadb { mariadb } else { mysql };
    let program = configs::database_home(settings, layout, db)?.join("bin").join(exe(name));
    let cnf = configs::write_client_config(settings, layout, db)?;
    Ok((program, vec![format!("--defaults-extra-file={}", cnf.display())]))
}

async fn output(program: &Path, args: &[String], stdin: Option<&Path>) -> Result<String> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(file) = stdin {
        cmd.stdin(std::fs::File::open(file)?);
    } else {
        cmd.stdin(Stdio::null());
    }
    crate::procgroup::prepare(&mut cmd);
    let out = cmd.output().await.map_err(|e| Error::msg(format!("Couldn't run {}: {e}", program.display())))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(Error::msg(if err.is_empty() { format!("{} failed", program.display()) } else { err }));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The databases on a server, without its system databases. The server must be running.
pub async fn databases(settings: &Settings, layout: &Layout, engine: DbEngine) -> Result<Vec<String>> {
    if engine == DbEngine::Sqlite {
        return Ok(sqlite_databases(layout));
    }
    if engine == DbEngine::Mongodb {
        let version = settings.versions.get("mongosh").ok_or_else(|| Error::msg("Install mongosh to list MongoDB's databases (Services)."))?;
        let program = layout.bin("mongosh", version).join("bin").join(exe("mongosh"));
        let args = vec![
            "--quiet".into(),
            "--port".into(),
            settings.ports.mongodb.to_string(),
            "--eval".into(),
            "db.adminCommand({ listDatabases: 1, nameOnly: true }).databases.forEach(d => print(d.name))".into(),
        ];
        let text = output(&program, &args, None).await?;
        return Ok(text.lines().map(str::trim).filter(|d| !d.is_empty() && !MONGO_SYSTEM.contains(d)).map(str::to_string).collect());
    }
    let text = match mysql_family(engine) {
        Some(db) => {
            let (program, mut args) = mysql_tool(settings, layout, db, "mariadb", "mysql")?;
            args.extend(["-N".into(), "-e".into(), "SHOW DATABASES".into()]);
            output(&program, &args, None).await?
        }
        None => {
            let (program, mut args) = postgres_tool(settings, layout, "psql")?;
            args.extend(["-d".into(), "postgres".into(), "-tAc".into(), "SELECT datname FROM pg_database WHERE NOT datistemplate AND datname <> 'postgres' ORDER BY 1".into()]);
            output(&program, &args, None).await?
        }
    };
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|d| !d.is_empty() && !MYSQL_SYSTEM.contains(&d.to_lowercase().as_str()))
        .map(str::to_string)
        .collect())
}

/// A database name that is safe in a file name and in the quoted SQL below.
fn check_name(database: &str) -> Result<()> {
    let ok = !database.is_empty() && database.len() <= 64 && database.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok { Ok(()) } else { Err(Error::msg("Database names for backups can use letters, numbers, dashes and underscores.")) }
}

/// Dumps one database into backups/<engine>/<database>-<date>.<sql|dump>.
pub async fn create(settings: &Settings, layout: &Layout, engine: DbEngine, database: &str) -> Result<Backup> {
    if engine != DbEngine::Sqlite {
        check_name(database)?;
    }
    let dir = folder(layout).join(engine.id());
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono_stamp();
    let result = match mysql_family(engine) {
        // "shop/database/database.sqlite" → shop~database~database.sqlite-<stamp>.sqlite
        None if engine == DbEngine::Sqlite => {
            let source = sqlite_file(layout, database)?;
            let path = dir.join(format!("{}-{stamp}.sqlite", database.replace('/', "~")));
            match sqlite_shell(settings, layout) {
                Some(shell) => {
                    let args = [source.display().to_string(), format!(".backup '{}'", path.display())];
                    output(&shell, &args, None).await.map(|_| path)
                }
                None if sqlite_side(&source, "-wal").exists() => Err(Error::msg(
                    "This database has changes in its WAL file that a plain copy would miss: install SQLite on the Services page (Databases → SQLite), then back it up again.",
                )),
                None => std::fs::copy(&source, &path).map(|_| path).map_err(Error::from),
            }
        }
        // A gzipped archive of the one database: mongorestore can rename it on the way back.
        None if engine == DbEngine::Mongodb => {
            let path = dir.join(format!("{database}-{stamp}.archive.gz"));
            let (program, mut args) = mongo_tool(settings, layout, "mongodump")?;
            args.extend([format!("--db={database}"), format!("--archive={}", path.display()), "--gzip".into()]);
            output(&program, &args, None).await.map(|_| path)
        }
        Some(db) => {
            let path = dir.join(format!("{database}-{stamp}.sql"));
            let (program, mut args) = mysql_tool(settings, layout, db, "mariadb-dump", "mysqldump")?;
            // Without --databases, so it can be restored under another name too.
            args.extend([
                "--single-transaction".into(),
                "--routines".into(),
                "--triggers".into(),
                "--events".into(),
                format!("--result-file={}", path.display()),
                database.into(),
            ]);
            output(&program, &args, None).await.map(|_| path)
        }
        None => {
            // pg_dump's custom format: compressed, and pg_restore can clean what it replaces.
            let path = dir.join(format!("{database}-{stamp}.dump"));
            let (program, mut args) = postgres_tool(settings, layout, "pg_dump")?;
            args.extend(["-F".into(), "c".into(), "-f".into(), path.display().to_string(), "-d".into(), database.into()]);
            output(&program, &args, None).await.map(|_| path)
        }
    };
    let path = result.map_err(|e| Error::msg(format!("Backing up {database} failed: {e}")))?;
    describe(engine, &path).ok_or_else(|| Error::msg("The backup file wasn't written"))
}

/// Restores a backup into `database` (created when it doesn't exist), replacing the
/// tables the backup contains.
pub async fn restore(settings: &Settings, layout: &Layout, engine: DbEngine, file: &Path, database: &str) -> Result<()> {
    if engine != DbEngine::Sqlite {
        check_name(database)?;
    }
    if !file.starts_with(folder(layout)) || !file.is_file() {
        return Err(Error::msg("Pick a backup from LampRabit's backups folder."));
    }
    match mysql_family(engine) {
        // The file is replaced as a whole, as the other engines replace the tables.
        None if engine == DbEngine::Sqlite => {
            let target = sqlite_file(layout, database)?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            remove_sqlite_journal(&target)?;
            std::fs::copy(file, &target)
                .map_err(|e| Error::msg(format!("Couldn't write {}: {e}. Stop the app that uses it, then try again.", target.display())))?;
        }
        None if engine == DbEngine::Mongodb => {
            let original = describe(engine, file).map(|b| b.database).unwrap_or_else(|| database.to_string());
            let (program, mut args) = mongo_tool(settings, layout, "mongorestore")?;
            args.extend([
                format!("--archive={}", file.display()),
                "--gzip".into(),
                "--drop".into(),
                format!("--nsFrom={original}.*"),
                format!("--nsTo={database}.*"),
            ]);
            output(&program, &args, None).await?;
        }
        Some(db) => {
            let (program, base) = mysql_tool(settings, layout, db, "mariadb", "mysql")?;
            let mut create = base.clone();
            create.extend(["-e".into(), format!("CREATE DATABASE IF NOT EXISTS `{database}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci")]);
            output(&program, &create, None).await?;
            let mut load = base;
            load.push(database.into());
            output(&program, &load, Some(file)).await?;
        }
        None => {
            let (createdb, base) = postgres_tool(settings, layout, "createdb")?;
            let exists = databases(settings, layout, engine).await?.iter().any(|d| d == database);
            if !exists {
                let mut args = base.clone();
                args.extend(["-E".into(), "UTF8".into(), "-T".into(), "template0".into(), database.into()]);
                output(&createdb, &args, None).await?;
            }
            let (pg_restore, mut args) = postgres_tool(settings, layout, "pg_restore")?;
            // --no-owner: the backup may come from another user's server.
            args.extend(["--clean".into(), "--if-exists".into(), "--no-owner".into(), "-d".into(), database.into(), file.display().to_string()]);
            output(&pg_restore, &args, None).await?;
        }
    }
    Ok(())
}

pub fn delete(layout: &Layout, file: &Path) -> Result<()> {
    if !file.starts_with(folder(layout)) {
        return Err(Error::msg("Only backups in LampRabit's backups folder can be deleted here."));
    }
    std::fs::remove_file(file)?;
    Ok(())
}

/// Every backup, newest first.
pub fn list(layout: &Layout) -> Vec<Backup> {
    let mut out = Vec::new();
    for engine in [DbEngine::Mariadb, DbEngine::Mysql, DbEngine::Postgres, DbEngine::Mongodb, DbEngine::Sqlite] {
        let Ok(entries) = std::fs::read_dir(folder(layout).join(engine.id())) else { continue };
        out.extend(entries.flatten().filter_map(|e| describe(engine, &e.path())));
    }
    out.sort_by(|a, b| b.created.cmp(&a.created));
    out
}

fn describe(engine: DbEngine, path: &Path) -> Option<Backup> {
    let meta = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    // shop-20261101-120000.sql, .dump or .archive.gz
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".archive.gz").or_else(|| path.file_stem()?.to_str())?;
    // "<database>-YYYYMMDD-HHMMSS"
    let database = stem.rsplitn(3, '-').nth(2)?.to_string();
    let database = if engine == DbEngine::Sqlite { database.replace('~', "/") } else { database };
    let created = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64;
    Some(Backup { engine, database, path: path.to_path_buf(), size: meta.len(), created })
}

/// YYYYMMDD-HHMMSS in UTC, without a date library.
fn chrono_stamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}{month:02}{day:02}-{:02}{:02}{:02}", rest / 3600, rest % 3600 / 60, rest % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_names_keep_the_database_name() {
        let dir = std::env::temp_dir().join(format!("lr-backups-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("my_shop-20261101-120000.sql");
        std::fs::write(&file, "--").unwrap();
        assert_eq!(describe(DbEngine::Mariadb, &file).unwrap().database, "my_shop");
        let file = dir.join("blog-2-20261101-120000.dump");
        std::fs::write(&file, "--").unwrap();
        assert_eq!(describe(DbEngine::Postgres, &file).unwrap().database, "blog-2", "dashes in names survive");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(chrono_stamp().len(), 15);
        assert!(check_name("a`b").is_err() && check_name("shop_1").is_ok());
    }

    #[test]
    fn sqlite_backups_find_and_name_project_files() {
        let root = std::env::temp_dir().join(format!("lr-sqlite-{}", std::process::id()));
        let layout = Layout::new(&root);
        let db = layout.www().join("shop").join("database");
        std::fs::create_dir_all(&db).unwrap();
        std::fs::write(db.join("database.sqlite"), "").unwrap();
        // Not SQLite, and inside vendor/: neither is listed.
        std::fs::write(db.join("notes.db"), "plain text, not a database").unwrap();
        let vendor = layout.www().join("shop").join("vendor");
        std::fs::create_dir_all(&vendor).unwrap();
        std::fs::write(vendor.join("fixture.sqlite"), "").unwrap();
        assert_eq!(sqlite_databases(&layout), ["shop/database/database.sqlite"]);

        let backup = root.join("shop~database~database.sqlite-20261101-120000.sqlite");
        std::fs::write(&backup, "").unwrap();
        assert_eq!(describe(DbEngine::Sqlite, &backup).unwrap().database, "shop/database/database.sqlite");

        assert!(sqlite_file(&layout, "../outside.sqlite").is_err());
        assert!(sqlite_file(&layout, "shop/.env").is_err());
        assert!(sqlite_file(&layout, "shop/database/database.sqlite").is_ok());
        let _ = std::fs::remove_dir_all(&root);
    }
}
