//! Command shims in `<root>/shims` (php, composer, mariadb, mysql…) that always run the
//! active versions with LampRabit's config, and optionally that folder on the user's PATH
//! so they work from any terminal.

use std::path::{Path, PathBuf};

use crate::{
    config::{Database, Settings},
    configs,
    error::{Error, Result},
    layout::{exe, Layout},
};

/// (shim name, MariaDB program, MySQL program). `mariadb*` always mean MariaDB; the
/// `mysql*` names follow Settings → Default database.
const DB_TOOLS: &[(&str, &str, &str)] = &[
    ("mysql", "mariadb", "mysql"),
    ("mysqldump", "mariadb-dump", "mysqldump"),
    ("mysqladmin", "mariadb-admin", "mysqladmin"),
];
const MARIADB_TOOLS: &[&str] = &["mariadb", "mariadb-dump", "mariadb-admin"];

/// Environment for processes that run language tools: JAVA_HOME for the active Java (Maven's
/// wrapper and Gradle look for it; the `java` shim alone isn't enough for them).
pub fn tool_env(settings: &Settings, layout: &Layout) -> Vec<(String, String)> {
    let mut env = Vec::new();
    if let Some(v) = settings.versions.get("java") {
        env.push(("JAVA_HOME".to_string(), path(&layout.bin("java", v))));
    }
    // Cargo runs rustc and friends itself; they need the same homes as the shims give it.
    if let Some(v) = settings.versions.get("rust") {
        env.extend(crate::runtimes::rust_env(layout, v));
    }
    env
}

pub fn dir(layout: &Layout) -> PathBuf {
    layout.root().join("shims")
}

/// Rewrites every shim for the current active versions; tools that aren't installed get none.
pub fn write(settings: &Settings, layout: &Layout) -> Result<PathBuf> {
    let dir = dir(layout);
    std::fs::create_dir_all(&dir)?;
    for entry in std::fs::read_dir(&dir)?.flatten() {
        let _ = std::fs::remove_file(entry.path());
    }
    // Sites with their own PHP version get it when the command runs inside their folder.
    let mut php_cases: Vec<Case> = settings
        .sites
        .iter()
        .filter_map(|(name, site)| Some((layout.www().join(name), site.php.as_ref()?)))
        .filter(|(_, v)| Some(*v) != settings.versions.php.as_ref())
        .filter_map(|(folder, v)| php_case(settings, layout, v, Some(folder)))
        .collect();
    php_cases.extend(settings.versions.php.as_ref().and_then(|v| php_case(settings, layout, v, None)));
    if php_cases.iter().any(|c| c.folder.is_none()) {
        shim(&dir, "php", &php_cases)?;
        let phar = settings.versions.composer.as_ref().map(|v| layout.bin("composer", v).join("composer.phar"));
        if let Some(phar) = phar.filter(|p| p.exists()) {
            for case in &mut php_cases {
                case.args.push(path(&phar));
            }
            shim(&dir, "composer", &php_cases)?;
        }
    }
    // The tools read host, port and user from client.cnf, so they reach LampRabit's
    // server as root without any options.
    let db_tool = |db: Database, program: &str| -> Option<Case> {
        let target = configs::database_home(settings, layout, db).ok()?.join("bin").join(exe(program));
        let cnf = configs::write_client_config(settings, layout, db).ok()?;
        target.exists().then(|| Case { folder: None, target, args: vec![format!("--defaults-extra-file={}", path(&cnf))], env: Vec::new() })
    };
    for name in MARIADB_TOOLS {
        if let Some(case) = db_tool(Database::Mariadb, name) {
            shim(&dir, name, &[case])?;
        }
    }
    if let Some(db) = settings.database() {
        for (name, mariadb, mysql) in DB_TOOLS {
            if let Some(case) = db_tool(db, if db == Database::Mariadb { mariadb } else { mysql }) {
                shim(&dir, name, &[case])?;
            }
        }
    }
    // The JDK's tools, with JAVA_HOME for build tools (Maven, Gradle) they start; each
    // project gets the Java its .java-version or .sdkmanrc asks for.
    for name in ["java", "javac", "jar", "jshell"] {
        let cases = language_cases(settings, layout, "java", |v, folder| {
            let home = layout.bin("java", v);
            let target = home.join("bin").join(exe(name));
            target.exists().then(|| Case { folder, target, args: Vec::new(), env: vec![("JAVA_HOME".to_string(), path(&home))] })
        });
        if cases.iter().any(|c| c.folder.is_none()) {
            shim(&dir, name, &cases)?;
        }
    }
    // Maven and Gradle on the active Java (their scripts find it through JAVA_HOME).
    if let Some(java) = settings.versions.get("java").map(|v| layout.bin("java", v)) {
        for (runtime, name, script) in [("maven", "mvn", if cfg!(windows) { "mvn.cmd" } else { "mvn" }), ("gradle", "gradle", if cfg!(windows) { "gradle.bat" } else { "gradle" })] {
            let Some(v) = settings.versions.get(runtime) else { continue };
            let target = layout.bin(runtime, v).join("bin").join(script);
            if target.exists() {
                shim(&dir, name, &[Case { folder: None, target, args: Vec::new(), env: vec![("JAVA_HOME".to_string(), path(&java))] }])?;
            }
        }
    }
        // mongosh, pointed at LampRabit's MongoDB when it isn't on the usual port.
    if let Some(home) = settings.versions.get("mongosh").map(|v| layout.bin("mongosh", v)) {
        let target = home.join("bin").join(exe("mongosh"));
        if target.exists() {
            let args = if settings.ports.mongodb == 27017 { Vec::new() } else { vec!["--port".to_string(), settings.ports.mongodb.to_string()] };
            shim(&dir, "mongosh", &[Case { folder: None, target, args, env: Vec::new() }])?;
        }
    }
    // MongoDB's dump and restore tools.
    if let Some(home) = settings.versions.get("mongotools").map(|v| layout.bin("mongotools", v)) {
        for name in ["mongodump", "mongorestore", "mongoexport", "mongoimport"] {
            let target = home.join("bin").join(exe(name));
            if target.exists() {
                shim(&dir, name, &[Case { folder: None, target, args: Vec::new(), env: Vec::new() }])?;
            }
        }
    }
    // sqlite3, and ClickHouse's client pointed at LampRabit's server.
    if let Some(v) = settings.versions.get("sqlite") {
        let target = layout.bin("sqlite", v).join(exe("sqlite3"));
        if target.exists() {
            shim(&dir, "sqlite3", &[Case { folder: None, target, args: Vec::new(), env: Vec::new() }])?;
        }
    }
    if let Some(v) = settings.versions.get("clickhouse") {
        let target = layout.bin("clickhouse", v).join("usr").join("bin").join("clickhouse");
        if target.exists() {
            let args = vec!["client".to_string(), "--port".to_string(), settings.ports.clickhouse_tcp.to_string()];
            shim(&dir, "clickhouse-client", &[Case { folder: None, target, args, env: Vec::new() }])?;
        }
    }
    // ollama's CLI, talking to LampRabit's Ollama port: LampRabit's own, else the one
    // installed on the computer.
    let ollama = match settings.versions.get("ollama") {
        Some(v) => Some(layout.bin("ollama", v).join(crate::runtimes::Runtime::Ollama.marker())),
        None => crate::runtimes::ollama_elsewhere(),
    };
    if let Some(target) = ollama {
        if target.exists() {
            let env = vec![("OLLAMA_HOST".to_string(), format!("127.0.0.1:{}", settings.ports.ollama))];
            shim(&dir, "ollama", &[Case { folder: None, target, args: Vec::new(), env }])?;
        }
    }
    // Go's tools (Go finds its own root from where it runs); each project gets the
    // installed Go its go.mod asks for.
    for name in ["go", "gofmt"] {
        let cases = language_cases(settings, layout, "go", |v, folder| {
            let target = layout.bin("go", v).join("bin").join(exe(name));
            target.exists().then(|| Case { folder, target, args: Vec::new(), env: Vec::new() })
        });
        if cases.iter().any(|c| c.folder.is_none()) {
            shim(&dir, name, &cases)?;
        }
    }
    // Rust's tools from LampRabit's rustup, with its homes (not the user's ~/.cargo).
    if let Some(v) = settings.versions.get("rust") {
        let env = crate::runtimes::rust_env(layout, v);
        for name in ["cargo", "rustc", "rustup", "rustfmt", "rustdoc"] {
            let target = layout.bin("rust", v).join("cargo").join("bin").join(exe(name));
            if target.exists() {
                shim(&dir, name, &[Case { folder: None, target, args: Vec::new(), env: env.clone() }])?;
            }
        }
    }
    // Python and its pip; each project gets the Python its .python-version asks for.
    for (name, args) in [("python", &[][..]), ("python3", &[][..]), ("pip", &["-m", "pip"][..])] {
        let cases = language_cases(settings, layout, "python", |v, folder| {
            let target = layout.bin("python", v).join(crate::runtimes::Runtime::Python.marker());
            target.exists().then(|| Case { folder, target, args: args.iter().map(|a| a.to_string()).collect(), env: Vec::new() })
        });
        if cases.iter().any(|c| c.folder.is_none()) {
            shim(&dir, name, &cases)?;
        }
    }
    // PostgreSQL's clients, pointed at LampRabit's server as its superuser.
    if let Some(home) = settings.versions.get("postgres").map(|v| layout.bin("postgres", v)) {
        let env = vec![
            ("PGHOST".to_string(), "127.0.0.1".to_string()),
            ("PGPORT".to_string(), settings.ports.postgres.to_string()),
            ("PGUSER".to_string(), "postgres".to_string()),
        ];
        for name in ["psql", "pg_dump", "pg_dumpall", "pg_restore", "createdb", "dropdb"] {
            let target = home.join("bin").join(exe(name));
            if target.exists() {
                shim(&dir, name, &[Case { folder: None, target, args: Vec::new(), env: env.clone() }])?;
            }
        }
    }
    // redis-cli from the active Redis, pointed at LampRabit's port.
    let cli = settings.versions.redis.as_ref().map(|v| layout.bin("redis", v).join(exe("redis-cli")));
    if let Some(target) = cli.filter(|t| t.exists()) {
        let args = vec!["-p".to_string(), settings.ports.redis.to_string()];
        shim(&dir, "redis-cli", &[Case { folder: None, target, args, env: Vec::new() }])?;
    }
    Ok(dir)
}

/// A language command's cases: each project asking (through its version file) for an
/// installed version other than the active one, then the active version for everywhere else.
fn language_cases(settings: &Settings, layout: &Layout, runtime: &str, case: impl Fn(&str, Option<PathBuf>) -> Option<Case>) -> Vec<Case> {
    let Some(rt) = crate::runtimes::Runtime::from_id(runtime) else { return Vec::new() };
    let installed = crate::runtimes::installed(layout, rt);
    let active = settings.versions.get(runtime).cloned();
    let mut cases: Vec<Case> = crate::langver::requests(layout, runtime)
        .into_iter()
        .filter_map(|(folder, req)| {
            let v = crate::langver::matching(&installed, &req.version)?;
            if Some(&v) == active.as_ref() {
                return None;
            }
            case(&v, Some(folder))
        })
        .collect();
    cases.extend(active.and_then(|v| case(&v, None)));
    cases
}

fn path(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// Run `target args…` when the working directory is inside `folder` (or anywhere, for None).
struct Case {
    folder: Option<PathBuf>,
    target: PathBuf,
    args: Vec<String>,
    /// Environment set before it runs, which programs it starts inherit.
    env: Vec<(String, String)>,
}

fn php_case(settings: &Settings, layout: &Layout, version: &str, folder: Option<PathBuf>) -> Option<Case> {
    let target = layout.bin("php", version).join(exe("php"));
    if !target.exists() {
        return None;
    }
    // PHPRC as well as -c: PHP that a script starts by itself (`php artisan serve` runs
    // `php -S`, Composer runs scripts) gets no -c, and without LampRabit's php.ini it has no
    // extensions (pdo_mysql: "could not find driver").
    let (args, env) = match configs::write_php(settings, layout, version, settings.ports.php_fpm) {
        Ok(ini) => (vec!["-c".to_string(), path(&ini)], ini.parent().map(|d| vec![("PHPRC".to_string(), path(d))]).unwrap_or_default()),
        Err(_) => (Vec::new(), Vec::new()),
    };
    Some(Case { folder, target, args, env })
}

/// Writes a shim that picks the first case whose folder contains the working directory;
/// the case without a folder is the fallback.
fn shim(dir: &Path, name: &str, cases: &[Case]) -> Result<()> {
    let fallback = cases.iter().find(|c| c.folder.is_none());
    if cfg!(windows) {
        let run = |c: &Case| {
            let args: String = c.args.iter().map(|a| format!(" \"{a}\"")).collect();
            let env: String = c.env.iter().map(|(k, v)| format!("set \"{k}={v}\"\r\n")).collect();
            format!("{env}\"{}\"{args} %*\r\nexit /b %ERRORLEVEL%\r\n", c.target.display())
        };
        let mut script = String::from("@echo off\r\nsetlocal\r\nset \"LR_HERE=%CD%\\\"\r\n");
        let mut labels = String::new();
        for (i, case) in cases.iter().enumerate() {
            if let Some(folder) = &case.folder {
                // Compare the start of %CD%\ with the folder, case-insensitively.
                let prefix = format!("{}\\", folder.display());
                let n = prefix.chars().count();
                script.push_str(&format!("if /I \"%LR_HERE:~0,{n}%\"==\"{prefix}\" goto site{i}\r\n"));
                labels.push_str(&format!(":site{i}\r\n{}", run(case)));
            }
        }
        if let Some(c) = fallback {
            script.push_str(&run(c));
        }
        script.push_str(&labels);
        std::fs::write(dir.join(format!("{name}.cmd")), script)?;
    } else {
        let quote = |s: &str| format!("'{}'", s.replace('\'', r"'\''"));
        let run = |c: &Case| {
            let args: String = c.args.iter().map(|a| format!(" {}", quote(a))).collect();
            let env: String = c.env.iter().map(|(k, v)| format!("{k}={} ", quote(v))).collect();
            let export = if env.is_empty() { String::new() } else { format!("export {}; ", env.trim_end()) };
            format!("{export}exec {}{args} \"$@\"", quote(&c.target.to_string_lossy()))
        };
        let mut script = String::from("#!/bin/sh\ncase \"$PWD/\" in\n");
        for case in cases {
            if let Some(folder) = &case.folder {
                script.push_str(&format!("  {}*) {} ;;\n", quote(&format!("{}/", folder.display())), run(case)));
            }
        }
        script.push_str("esac\n");
        if let Some(c) = fallback {
            script.push_str(&run(c));
            script.push('\n');
        }
        let file = dir.join(name);
        std::fs::write(&file, script)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(())
}

/// Adds or removes the shims folder on the user's PATH. New terminals see the change;
/// ones already open keep their old PATH.
pub fn set_on_path(layout: &Layout, on: bool) -> Result<()> {
    let dir = dir(layout);
    #[cfg(windows)]
    return windows_path::set(&dir, on);
    #[cfg(not(windows))]
    return profile::set(&dir, on);
}

#[cfg(windows)]
mod windows_path {
    use super::*;
    use winreg::{enums::*, RegKey, RegValue};

    /// Edits HKCU\Environment\Path directly (no elevation), keeping it REG_EXPAND_SZ so
    /// entries like %USERPROFILE%\bin survive, then tells running programs it changed.
    pub fn set(dir: &Path, on: bool) -> Result<()> {
        let env = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
            .map_err(|e| Error::msg(format!("Couldn't open your user environment: {e}")))?;
        let current: String = match env.get_raw_value("Path") {
            Ok(v) => decode(&v.bytes),
            Err(_) => String::new(),
        };
        let mine = dir.to_string_lossy();
        let mut parts: Vec<&str> =
            current.split(';').filter(|p| !p.is_empty() && !p.trim_end_matches('\\').eq_ignore_ascii_case(&mine)).collect();
        if on {
            parts.insert(0, &mine);
        }
        let value = parts.join(";");
        if value == current {
            return Ok(());
        }
        env.set_raw_value("Path", &RegValue { bytes: encode(&value), vtype: REG_EXPAND_SZ })
            .map_err(|e| Error::msg(format!("Couldn't update your PATH: {e}")))?;
        broadcast();
        Ok(())
    }

    fn decode(bytes: &[u8]) -> String {
        let wide: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&wide).trim_end_matches('\0').to_string()
    }

    fn encode(s: &str) -> Vec<u8> {
        s.encode_utf16().chain(std::iter::once(0)).flat_map(u16::to_le_bytes).collect()
    }

    fn broadcast() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
        };
        let name: Vec<u16> = "Environment".encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, name.as_ptr() as isize, SMTO_ABORTIFHUNG, 2000, std::ptr::null_mut());
        }
    }
}

#[cfg(not(windows))]
mod profile {
    use super::*;

    const START: &str = "# >>> LampRabit >>>";
    const END: &str = "# <<< LampRabit <<<";

    /// A managed block in the shell start-up files. zsh is macOS's default shell, so
    /// .zshrc is created there if missing; elsewhere only existing files are touched,
    /// falling back to .profile.
    pub fn set(dir: &Path, on: bool) -> Result<()> {
        let home = dirs::home_dir().ok_or_else(|| Error::msg("Couldn't find your home folder"))?;
        let mut files: Vec<PathBuf> =
            [".zshrc", ".bashrc", ".profile"].iter().map(|f| home.join(f)).filter(|f| f.exists()).collect();
        if on && cfg!(target_os = "macos") && !files.iter().any(|f| f.ends_with(".zshrc")) {
            files.push(home.join(".zshrc"));
        }
        if on && files.is_empty() {
            files.push(home.join(".profile"));
        }
        let block = format!("{START}\nexport PATH=\"{}:$PATH\"\n{END}\n", dir.display());
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            let mut kept = strip(&text);
            if on {
                if !kept.is_empty() && !kept.ends_with('\n') {
                    kept.push('\n');
                }
                kept.push_str(&block);
            }
            if kept != text {
                std::fs::write(&file, kept)?;
            }
        }
        Ok(())
    }

    fn strip(text: &str) -> String {
        let mut out = String::new();
        let mut inside = false;
        for line in text.split_inclusive('\n') {
            match line.trim_end() {
                START => inside = true,
                END => inside = false,
                _ if !inside => out.push_str(line),
                _ => {}
            }
        }
        out
    }
}
