//! Turning failures into explanations: who holds a port, what a crash log means, and
//! health probes for running services.

use std::time::Duration;

use serde::Serialize;
use tokio::{io::AsyncReadExt, net::TcpStream, process::Command};

use crate::procgroup;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortOwner {
    pub pid: u32,
    pub name: String,
    /// What this program usually is and how to free the port.
    pub note: Option<String>,
    /// Whether LampRabit may offer to stop it (never system processes).
    pub stoppable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub title: String,
    pub detail: String,
    pub fixes: Vec<String>,
    /// Set for port conflicts so the UI can offer to change or free the port.
    pub port: Option<u16>,
    pub owner: Option<PortOwner>,
    pub at: u64,
}

impl Problem {
    pub fn new(title: impl Into<String>, detail: impl Into<String>, fixes: &[&str]) -> Self {
        Self {
            title: title.into(),
            detail: detail.into(),
            fixes: fixes.iter().map(|f| f.to_string()).collect(),
            port: None,
            owner: None,
            at: now_ms(),
        }
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Finds the process listening on `port`, if the OS lets us see it.
pub async fn port_owner(port: u16) -> Option<PortOwner> {
    let (pid, name) = if cfg!(windows) {
        windows_owner(port).await?
    } else {
        unix_owner(port).await?
    };
    let lower = name.to_lowercase();
    let system = pid == 0
        || pid == 4
        || lower == "system"
        || lower == "svchost.exe"
        || pid == std::process::id();
    Some(PortOwner {
        note: owner_note(pid, &lower),
        stoppable: !system,
        pid,
        name,
    })
}

async fn output(program: &str, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    procgroup::prepare(&mut cmd);
    let out = tokio::time::timeout(Duration::from_secs(5), cmd.output())
        .await
        .ok()?
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

async fn windows_owner(port: u16) -> Option<(u32, String)> {
    // "TCP  0.0.0.0:80  0.0.0.0:0  LISTENING  4". The state word is localised, so a
    // listener is recognised by its wildcard foreign address instead.
    let netstat = output("netstat", &["-ano", "-p", "TCP"])
        .await
        .unwrap_or_default()
        + &output("netstat", &["-ano", "-p", "TCPv6"])
            .await
            .unwrap_or_default();
    let suffix = format!(":{port}");
    let pid: u32 = netstat.lines().find_map(|line| {
        let cols: Vec<&str> = line.split_whitespace().collect();
        let listening = cols.len() == 5 && (cols[2] == "0.0.0.0:0" || cols[2] == "[::]:0");
        (listening && cols[1].ends_with(&suffix))
            .then(|| cols[4].parse().ok())
            .flatten()
    })?;
    if pid == 4 {
        return Some((pid, "System".into()));
    }
    let filter = format!("PID eq {pid}");
    let list = output("tasklist", &["/FI", &filter, "/FO", "CSV", "/NH"])
        .await
        .unwrap_or_default();
    let name = list.split('"').nth(1).unwrap_or("unknown").to_string();
    Some((pid, name))
}

async fn unix_owner(port: u16) -> Option<(u32, String)> {
    let spec = format!("-iTCP:{port}");
    let out = output("lsof", &["-nP", &spec, "-sTCP:LISTEN", "-Fpc"]).await?;
    let pid = out
        .lines()
        .find_map(|l| l.strip_prefix('p')?.parse().ok())?;
    let name = out
        .lines()
        .find_map(|l| l.strip_prefix('c'))
        .unwrap_or("unknown")
        .to_string();
    Some((pid, name))
}

fn owner_note(pid: u32, name: &str) -> Option<String> {
    let note = if pid == 4 || name == "system" {
        "This is Windows' built-in HTTP server (http.sys), usually IIS / World Wide Web Publishing Service, \
         SQL Server Reporting Services or Web Deploy. Stop that service (or run `net stop http /y` in an \
         administrator terminal), or give LampRabit another port."
    } else if name.contains("httpd") || name.contains("apache") {
        "Another Apache is running, for example from XAMPP, Laragon or WAMP. Stop it before starting LampRabit's."
    } else if name.contains("nginx") {
        "An Nginx server is running."
    } else if name.contains("mysqld") || name.contains("mariadbd") {
        "Another MySQL or MariaDB server is running (XAMPP, Laragon or a MySQL Windows service)."
    } else if name.contains("skype") {
        "Older Skype versions take ports 80 and 443. Turn off \"Use port 80 and 443\" in Skype's settings."
    } else if name.contains("svchost") {
        "A Windows service is using this port."
    } else if name.contains("docker")
        || name.contains("vpnkit")
        || name.contains("wslrelay")
        || name.contains("com.docker")
    {
        "Docker or WSL is forwarding a container port here."
    } else if name.starts_with("node") {
        "A Node.js development server is running."
    } else {
        return None;
    };
    Some(note.to_string())
}

pub fn port_problem(service: &str, port: u16, owner: Option<PortOwner>) -> Problem {
    let who = owner
        .as_ref()
        .map(|o| format!("{} (PID {})", o.name, o.pid))
        .unwrap_or_else(|| "another program".into());
    let mut p = Problem::new(
        format!("Port {port} is already in use"),
        format!("{service} can't start because {who} is listening on port {port}."),
        &["Stop the other program, or change the port in Settings."],
    );
    if let Some(note) = owner.as_ref().and_then(|o| o.note.clone()) {
        p.fixes.insert(0, note);
    }
    p.port = Some(port);
    p.owner = owner;
    p
}

/// Known failure signatures in service logs, most specific first.
const PATTERNS: &[(&str, &str, &str)] = &[
    (
        "could not bind to address",
        "Apache couldn't open its port",
        "Another program took the port just before Apache started. Check Settings → Ports, or run diagnostics to see who holds it.",
    ),
    (
        "Syntax error on line",
        "Apache configuration error",
        "A config file has an invalid line. If it is in etc/apache/sites/, fix or remove that file; LampRabit's own files are regenerated on start.",
    ),
    (
        "php8apache2_4.dll",
        "Apache couldn't load PHP",
        "The PHP module failed to load. Install the Microsoft Visual C++ 2015–2022 Redistributable (x64), or switch to another PHP version.",
    ),
    (
        "Bind on TCP/IP port",
        "MariaDB couldn't open its port",
        "Another database server is probably running. Stop it or change the MariaDB port in Settings.",
    ),
    (
        "Plugin 'InnoDB' init function returned error",
        "MariaDB couldn't open its data files",
        "This usually means the data directory was created by a different MariaDB major version, or was damaged. Switch back to the previous version, or move data/mariadb aside to start fresh.",
    ),
    (
        "Unknown/unsupported storage engine",
        "MariaDB couldn't open its data files",
        "The InnoDB engine failed to start; see the lines above it in the MariaDB log.",
    ),
    (
        "Only one usage of each socket address",
        "Port already in use",
        "Another program is listening on this port. Change it in Settings or stop that program.",
    ),
    (
        "address already in use",
        "Port already in use",
        "Another program is listening on this port. Change it in Settings or stop that program.",
    ),
];

/// Explains why a service stopped, from its exit code and the tail of its logs.
pub fn explain_exit(service: &str, exit_code: Option<i32>, log_tail: &str) -> Problem {
    // STATUS_DLL_NOT_FOUND / STATUS_INVALID_IMAGE_FORMAT
    if matches!(exit_code, Some(-1073741515) | Some(-1073741701)) {
        return Problem::new(
            format!("{service} is missing system libraries"),
            "Windows couldn't load a DLL the program needs.",
            &[
                "Install the Microsoft Visual C++ 2015–2022 Redistributable (x64) from Microsoft, then start again.",
            ],
        );
    }
    let lower = log_tail.to_lowercase();
    for (needle, title, fix) in PATTERNS {
        if lower.contains(&needle.to_lowercase()) {
            let line = log_tail
                .lines()
                .rev()
                .find(|l| l.to_lowercase().contains(&needle.to_lowercase()))
                .unwrap_or_default()
                .trim();
            return Problem::new(*title, line, &[fix]);
        }
    }
    let last_error = log_tail
        .lines()
        .rev()
        .find(|l| {
            let l = l.to_lowercase();
            l.contains("error") || l.contains("fatal") || l.contains("emerg") || l.contains("crit")
        })
        .unwrap_or_default()
        .trim()
        .to_string();
    Problem::new(
        format!("{service} stopped unexpectedly"),
        if last_error.is_empty() {
            format!(
                "It exited with code {}.",
                exit_code.map_or("unknown".into(), |c| c.to_string())
            )
        } else {
            last_error
        },
        &["Open the Logs page for the full output, or run diagnostics."],
    )
}

/// The last `bytes` of a file, starting at a line boundary.
pub fn tail(path: &std::path::Path, bytes: u64) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else {
        return String::new();
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let from = len.saturating_sub(bytes);
    let mut buf = Vec::new();
    if f.seek(SeekFrom::Start(from)).is_err() || f.read_to_end(&mut buf).is_err() {
        return String::new();
    }
    let text = String::from_utf8_lossy(&buf).into_owned();
    match (from > 0, text.find('\n')) {
        (true, Some(i)) => text[i + 1..].to_string(),
        _ => text,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    Ok,
    Bad(String),
}

pub async fn probe_http(client: &reqwest::Client, url: &str) -> Probe {
    match client.get(url).timeout(Duration::from_secs(3)).send().await {
        Ok(r) if r.status().is_server_error() => {
            Probe::Bad(format!("Answers with HTTP {}", r.status().as_u16()))
        }
        // Read the page before hanging up: Caddy logs a warning for every response the
        // client abandons halfway, which would fill its log every few seconds.
        Ok(r) => {
            let _ = tokio::time::timeout(Duration::from_secs(2), r.bytes()).await;
            Probe::Ok
        }
        Err(e) if e.is_timeout() => Probe::Bad(format!("No answer from {url} within 3 seconds")),
        Err(_) => Probe::Bad(format!("Not answering at {url}")),
    }
}

/// Sends a one-line command and expects a reply starting with `expect` (Redis PING,
/// Memcached version).
pub async fn probe_line(port: u16, command: &[u8], expect: &str) -> Probe {
    use tokio::io::AsyncWriteExt;
    let attempt = async {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.ok()?;
        stream.write_all(command).await.ok()?;
        let mut buf = [0u8; 128];
        let n = stream.read(&mut buf).await.ok()?;
        Some(String::from_utf8_lossy(&buf[..n]).to_string())
    };
    match tokio::time::timeout(std::time::Duration::from_secs(3), attempt).await {
        Ok(Some(reply)) if reply.starts_with(expect) => Probe::Ok,
        Ok(Some(reply)) => Probe::Bad(format!("Unexpected reply: {}", reply.trim())),
        _ => Probe::Bad(format!("Not accepting connections on port {port}")),
    }
}

/// MySQL/MariaDB greet every new connection; a greeting means the server is ready,
/// an error packet carries the reason it isn't (e.g. "Too many connections").
/// Whether something accepts connections on the port (MongoDB, whose wire protocol needs
/// a full message to say more).
pub async fn probe_tcp(port: u16) -> Probe {
    match tokio::time::timeout(
        Duration::from_secs(3),
        TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    {
        Ok(Ok(_)) => Probe::Ok,
        _ => Probe::Bad(format!("Not accepting connections on port {port}")),
    }
}

/// Asks PostgreSQL whether it would speak TLS (an SSLRequest): any server answers with a
/// single `S` or `N` before authentication, so this needs no user or password.
pub async fn probe_postgres(port: u16) -> Probe {
    use tokio::io::AsyncWriteExt;
    let attempt = async {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.ok()?;
        stream.write_all(&[0, 0, 0, 8, 4, 210, 22, 47]).await.ok()?;
        let mut reply = [0u8; 1];
        stream.read_exact(&mut reply).await.ok()?;
        Some(reply[0])
    };
    match tokio::time::timeout(Duration::from_secs(3), attempt).await {
        Ok(Some(b'S' | b'N')) => Probe::Ok,
        Ok(Some(_)) => Probe::Bad("Unexpected reply to a new connection".into()),
        _ => Probe::Bad(format!("Not accepting connections on port {port}")),
    }
}

pub async fn probe_mysql(port: u16) -> Probe {
    let attempt = async {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.ok()?;
        let mut buf = [0u8; 256];
        let n = stream.read(&mut buf).await.ok()?;
        Some(buf[..n].to_vec())
    };
    match tokio::time::timeout(Duration::from_secs(3), attempt).await {
        Ok(Some(packet)) if packet.len() > 4 && packet[4] == 0x0a => Probe::Ok,
        Ok(Some(packet)) if packet.len() > 7 && packet[4] == 0xff => Probe::Bad(format!(
            "Refusing connections: {}",
            String::from_utf8_lossy(&packet[7..]).trim()
        )),
        Ok(Some(_)) => Probe::Bad("Unexpected reply to a new connection".into()),
        _ => Probe::Bad(format!("Not accepting connections on port {port}")),
    }
}
