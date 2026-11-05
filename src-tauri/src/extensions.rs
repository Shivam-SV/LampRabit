//! PHP extension manager: bundled extensions per PHP version can be switched on or off,
//! and popular extras (Xdebug, Redis, APCu, ...) installed from their Windows builds.
//! Compatibility comes from what actually exists for download for that PHP version.

use std::{collections::BTreeSet, path::PathBuf};

use serde::Serialize;

use crate::{
    config::Settings,
    error::{Error, Result},
    layout::Layout,
};

/// Enabled by default when the PHP build ships them.
pub const DEFAULTS: &[&str] = &[
    "bz2", "curl", "exif", "fileinfo", "gd", "gettext", "intl", "mbstring", "mysqli", "openssl", "pdo_mysql", "pdo_pgsql", "pdo_sqlite", "pgsql",
    "sodium", "sqlite3", "xsl", "zip", "opcache",
];

/// Extensions loaded with `zend_extension=` rather than `extension=`.
const ZEND: &[&str] = &["opcache", "xdebug"];

struct Extra {
    name: &'static str,
    description: &'static str,
    /// Extra php.ini lines when enabled.
    ini: &'static str,
    note: Option<&'static str>,
}

const EXTRAS: &[Extra] = &[
    Extra {
        name: "xdebug",
        description: "Step debugging, profiling and better error pages",
        ini: "xdebug.mode = debug,develop\nxdebug.start_with_request = trigger\nxdebug.client_port = 9003\n",
        note: Some("Debugging starts when the request has XDEBUG_TRIGGER (browser extension or ?XDEBUG_TRIGGER=1); your IDE listens on port 9003. Slows PHP down while enabled."),
    },
    Extra { name: "redis", description: "Native Redis client (phpredis)", ini: "", note: Some("Pairs with LampRabit's Redis service. Some apps also want igbinary.") },
    Extra { name: "apcu", description: "In-memory user cache", ini: "apc.enable_cli = 1\n", note: None },
    Extra { name: "igbinary", description: "Compact binary serializer, used by Redis and Memcached clients", ini: "", note: None },
    Extra {
        name: "memcached",
        description: "Memcached client (libmemcached)",
        ini: "",
        note: Some("Pairs with LampRabit's Memcached service. Installs libmemcached's DLL next to PHP."),
    },
    Extra { name: "mongodb", description: "MongoDB driver", ini: "", note: Some("Pairs with LampRabit's MongoDB service.") },
    Extra { name: "pcov", description: "Fast code coverage for PHPUnit", ini: "pcov.enabled = 1\n", note: Some("Don't enable together with Xdebug coverage.") },
    Extra { name: "imagick", description: "ImageMagick image processing", ini: "", note: Some("Installs ImageMagick's DLLs next to PHP.") },
    Extra { name: "yaml", description: "YAML parsing and emitting", ini: "", note: None },
];

fn describe(name: &str) -> Option<&'static str> {
    Some(match name {
        "bz2" => "bzip2 compression",
        "curl" => "HTTP requests with libcurl",
        "exif" => "Read image metadata",
        "fileinfo" => "Detect file types",
        "ftp" => "FTP client",
        "gd" => "Image creation and editing",
        "gettext" => "Translations",
        "gmp" => "Arbitrary-precision maths",
        "intl" => "Internationalisation (ICU)",
        "ldap" => "LDAP directory access",
        "mbstring" => "Multibyte (UTF-8) strings",
        "mysqli" => "MySQL/MariaDB client",
        "odbc" => "ODBC databases",
        "opcache" => "Bytecode cache (faster PHP)",
        "openssl" => "TLS and cryptography",
        "pdo_mysql" => "PDO driver for MySQL/MariaDB",
        "pdo_odbc" => "PDO driver for ODBC",
        "pdo_pgsql" => "PDO driver for PostgreSQL",
        "pdo_sqlite" => "PDO driver for SQLite",
        "pgsql" => "PostgreSQL client",
        "shmop" => "Shared memory",
        "soap" => "SOAP web services",
        "sockets" => "Low-level sockets",
        "sodium" => "Modern cryptography (libsodium)",
        "sqlite3" => "SQLite databases",
        "tidy" => "Clean up HTML",
        "xsl" => "XSLT transformations",
        "zip" => "Read and write zip archives",
        "ffi" => "Call C libraries from PHP",
        "com_dotnet" => "COM and .NET (Windows)",
        "enchant" => "Spell checking",
        "dba" => "Database-style key/value files",
        "snmp" => "SNMP network management",
        "sysvshm" => "System V shared memory",
        _ => return None,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtInfo {
    pub name: String,
    /// "bundled" with this PHP build, or an "extra" to download.
    pub kind: &'static str,
    pub description: Option<String>,
    pub enabled: bool,
    pub installed: bool,
    /// For extras: the version downloadable for this PHP, None when there's no build.
    pub available: Option<String>,
    pub note: Option<String>,
}

fn ext_dir(layout: &Layout, php: &str) -> PathBuf {
    layout.bin("php", php).join("ext")
}

fn present(layout: &Layout, php: &str) -> BTreeSet<String> {
    std::fs::read_dir(ext_dir(layout, php))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().into_string().ok()?;
            Some(n.strip_prefix("php_")?.strip_suffix(".dll")?.to_string())
        })
        .collect()
}

/// The extensions php.ini should load for a PHP version: defaults that the build ships,
/// plus what the user switched on, minus what they switched off.
pub fn effective(settings: &Settings, layout: &Layout, php: &str) -> Vec<String> {
    let present = present(layout, php);
    let overrides = settings.php_extensions.get(php);
    let mut on: BTreeSet<String> = DEFAULTS.iter().map(|s| s.to_string()).filter(|e| present.contains(e)).collect();
    if let Some(o) = overrides {
        on.extend(o.enabled.iter().filter(|e| present.contains(*e)).cloned());
        for off in &o.disabled {
            on.remove(off);
        }
    }
    on.into_iter().collect()
}

/// OPcache for development: on, but picks up edited files immediately.
const OPCACHE_DEV: &str = "opcache.enable=1\nopcache.validate_timestamps=1\nopcache.revalidate_freq=0\n";

/// php.ini lines for the enabled extensions.
pub fn ini_lines(settings: &Settings, layout: &Layout, php: &str) -> String {
    let mut out = String::new();
    if !cfg!(windows) {
        // LampRabit's Linux builds have every extension compiled in; only settings apply.
        out.push_str(OPCACHE_DEV);
        return out;
    }
    for ext in effective(settings, layout, php) {
        let directive = if ZEND.contains(&ext.as_str()) { "zend_extension" } else { "extension" };
        out.push_str(&format!("{directive}={ext}\n"));
        if ext == "opcache" {
            out.push_str(OPCACHE_DEV);
        }
        if let Some(extra) = EXTRAS.iter().find(|x| x.name == ext) {
            out.push_str(extra.ini);
        }
    }
    out
}

fn php_line(php: &str) -> String {
    php.split('.').take(2).collect::<Vec<_>>().join(".")
}

async fn text(client: &reqwest::Client, url: &str) -> Option<String> {
    client.get(url).send().await.ok()?.error_for_status().ok()?.text().await.ok()
}

fn newest_version_dir(listing: &str) -> Option<String> {
    let mut versions: Vec<String> = listing
        .split("href=\"")
        .skip(1)
        .filter_map(|s| s.split('"').next())
        .map(|h| h.trim_end_matches('/').to_string())
        .filter(|v| v.split('.').count() == 3 && v.split('.').all(|p| p.parse::<u32>().is_ok()))
        .collect();
    versions.sort_by_key(|v| v.split('.').map(|p| p.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>());
    versions.pop()
}

/// Where an extra can be downloaded for this PHP version (Windows, thread-safe x64).
async fn download_url(client: &reqwest::Client, name: &str, php: &str) -> Option<(String, String)> {
    let line = php_line(php);
    if name == "xdebug" {
        let page = text(client, "https://xdebug.org/download").await?;
        let file = page
            .split(|c: char| c == '"' || c == '\'' || c == '>' || c == '<' || c == '/')
            .find(|t| t.starts_with("php_xdebug-") && t.contains(&format!("-{line}-ts-")) && t.ends_with("-x86_64.dll"))?
            .to_string();
        let version = file.strip_prefix("php_xdebug-")?.split('-').next()?.to_string();
        return Some((format!("https://xdebug.org/files/{file}"), version));
    }
    let base = format!("https://downloads.php.net/~windows/pecl/releases/{name}/");
    let version = newest_version_dir(&text(client, &base).await?)?;
    let listing = text(client, &format!("{base}{version}/")).await?;
    let file = listing
        .split("href=\"")
        .skip(1)
        .filter_map(|s| s.split('"').next())
        .map(|h| h.rsplit('/').next().unwrap_or(h).to_string())
        .find(|f| f.starts_with(&format!("php_{name}-{version}-{line}-ts-")) && f.ends_with("-x64.zip"))?;
    Some((format!("{base}{version}/{file}"), version))
}

pub async fn list(client: &reqwest::Client, settings: &Settings, layout: &Layout, php: &str) -> Result<Vec<ExtInfo>> {
    if !cfg!(windows) {
        return Err(Error::msg("The extension manager works with LampRabit's Windows PHP builds for now."));
    }
    if !layout.bin("php", php).exists() {
        return Err(Error::msg(format!("PHP {php} is not installed")));
    }
    let present = present(layout, php);
    let on: BTreeSet<String> = effective(settings, layout, php).into_iter().collect();
    let extra_names: BTreeSet<&str> = EXTRAS.iter().map(|x| x.name).collect();

    let mut out: Vec<ExtInfo> = present
        .iter()
        .filter(|n| !extra_names.contains(n.as_str()))
        .map(|n| ExtInfo {
            name: n.clone(),
            kind: "bundled",
            description: describe(n).map(str::to_string),
            enabled: on.contains(n),
            installed: true,
            available: None,
            note: None,
        })
        .collect();

    let lookups = EXTRAS.iter().map(|x| async move { (x, download_url(client, x.name, php).await) });
    for (x, found) in futures_util::future::join_all(lookups).await {
        out.push(ExtInfo {
            name: x.name.to_string(),
            kind: "extra",
            description: Some(x.description.to_string()),
            enabled: on.contains(x.name),
            installed: present.contains(x.name),
            available: found.map(|(_, v)| v),
            note: x.note.map(str::to_string),
        });
    }
    Ok(out)
}

/// Downloads an extra into PHP's ext/ folder (and any DLLs it depends on next to php.exe).
pub async fn install(client: &reqwest::Client, layout: &Layout, php: &str, name: &str) -> Result<String> {
    if !EXTRAS.iter().any(|x| x.name == name) {
        return Err(Error::msg(format!("{name} isn't an extension LampRabit can install")));
    }
    let (url, version) = download_url(client, name, php)
        .await
        .ok_or_else(|| Error::msg(format!("There's no {name} build for PHP {} yet.", php_line(php))))?;
    let bytes = client
        .get(&url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| Error::msg(format!("Download failed: {e}")))?
        .bytes()
        .await
        .map_err(|e| Error::msg(format!("Download failed: {e}")))?;
    let ext = ext_dir(layout, php);
    let php_home = layout.bin("php", php);
    if url.ends_with(".dll") {
        std::fs::write(ext.join(format!("php_{name}.dll")), &bytes)?;
    } else {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).map_err(|e| Error::msg(format!("Bad download: {e}")))?;
        let main = format!("php_{name}.dll");
        for i in 0..zip.len() {
            let mut file = zip.by_index(i).map_err(|e| Error::msg(e.to_string()))?;
            let Some(file_name) = file.enclosed_name().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string())) else { continue };
            if !file_name.to_ascii_lowercase().ends_with(".dll") {
                continue;
            }
            // The extension goes in ext/; its dependencies beside php.exe so Windows finds them.
            let target = if file_name.eq_ignore_ascii_case(&main) { ext.join(&main) } else { php_home.join(&file_name) };
            let mut out = std::fs::File::create(target)?;
            std::io::copy(&mut file, &mut out)?;
        }
        if !ext.join(&main).exists() {
            return Err(Error::msg(format!("The {name} download didn't contain {main}")));
        }
    }
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_newest_pecl_version() {
        let listing = r#"<a href="5.1.9/">5.1.9/</a> <a href="5.1.28/">5.1.28/</a> <a href="5.1.27/">x</a> <a href="../">..</a>"#;
        assert_eq!(newest_version_dir(listing).as_deref(), Some("5.1.28"));
    }
}
