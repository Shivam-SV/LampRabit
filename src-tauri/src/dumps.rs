//! The dump catcher: PHP's `dump()` and `dd()` (Symfony VarDumper, which Laravel uses too)
//! shown in Logs instead of the page.
//!
//! VarDumper has a "server" mode made for this: with `VAR_DUMPER_FORMAT=server` it sends each
//! dump to a TCP address as one line, `base64(serialize([$data, $context]))`, and falls back to
//! dumping into the page when nothing listens. LampRabit's php.ini sets that through a small
//! `auto_prepend_file` (web requests only, so `php artisan tinker` keeps printing in the
//! terminal), listens on `ports.dumps`, and decodes the payload here: a PHP `unserialize`
//! reader, then VarDumper's `Data` (a table of items and `Stub`s) turned into a tree.

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Mutex,
};

use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};

/// Dumps kept for the Logs page.
const KEEP: usize = 300;
/// Nodes rendered per dump, so a huge object graph can't stall the app.
const MAX_NODES: usize = 4000;
const MAX_DEPTH: usize = 24;

// ---------------------------------------------------------------------------------------
// PHP's serialize() format.

#[derive(Debug, Clone, PartialEq)]
pub enum Php {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Vec<u8>),
    Array(Vec<(Php, Php)>),
    Object { class: String, props: Vec<(Php, Php)> },
    /// `r:`/`R:` back-references; shown as "(reference)" rather than resolved.
    Ref,
    Enum(String),
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn byte(&mut self, want: u8) -> Option<()> {
        (self.bytes.get(self.at) == Some(&want)).then(|| self.at += 1)
    }

    /// The bytes up to `end`, which is consumed.
    fn until(&mut self, end: u8) -> Option<&'a [u8]> {
        let rest = &self.bytes[self.at..];
        let n = rest.iter().position(|&b| b == end)?;
        self.at += n + 1;
        Some(&rest[..n])
    }

    fn number<T: std::str::FromStr>(&mut self, end: u8) -> Option<T> {
        std::str::from_utf8(self.until(end)?).ok()?.parse().ok()
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let out = self.bytes.get(self.at..self.at + n)?;
        self.at += n;
        Some(out)
    }

    /// `len:"text"` (the class names of `O:` and `E:`).
    fn quoted(&mut self) -> Option<&'a [u8]> {
        let len = self.number::<usize>(b':')?;
        self.byte(b'"')?;
        let text = self.take(len)?;
        self.byte(b'"')?;
        Some(text)
    }

    fn pairs(&mut self, depth: usize) -> Option<Vec<(Php, Php)>> {
        let n = self.number::<usize>(b':')?;
        self.byte(b'{')?;
        let mut out = Vec::with_capacity(n.min(1024));
        for _ in 0..n {
            let k = self.value(depth + 1)?;
            let v = self.value(depth + 1)?;
            out.push((k, v));
        }
        self.byte(b'}')?;
        Some(out)
    }

    fn value(&mut self, depth: usize) -> Option<Php> {
        if depth > 512 {
            return None;
        }
        let kind = *self.bytes.get(self.at)?;
        self.at += 1;
        if kind == b'N' {
            self.byte(b';')?;
            return Some(Php::Null);
        }
        self.byte(b':')?;
        Some(match kind {
            b'b' => Php::Bool(self.until(b';')? == b"1"),
            b'i' => Php::Int(self.number(b';')?),
            b'd' => {
                let text = std::str::from_utf8(self.until(b';')?).ok()?;
                Php::Float(match text {
                    "INF" => f64::INFINITY,
                    "-INF" => f64::NEG_INFINITY,
                    "NAN" => f64::NAN,
                    t => t.parse().ok()?,
                })
            }
            b's' => {
                let len = self.number::<usize>(b':')?;
                self.byte(b'"')?;
                let s = self.take(len)?.to_vec();
                self.byte(b'"')?;
                self.byte(b';')?;
                Php::Str(s)
            }
            b'a' => Php::Array(self.pairs(depth)?),
            b'O' => {
                let class = String::from_utf8_lossy(self.quoted()?).into_owned();
                self.byte(b':')?;
                Php::Object { class, props: self.pairs(depth)? }
            }
            b'C' => {
                // A class with its own Serializable format: kept as its class name only.
                let class = String::from_utf8_lossy(self.quoted()?).into_owned();
                self.byte(b':')?;
                let len = self.number::<usize>(b':')?;
                self.byte(b'{')?;
                self.take(len)?;
                self.byte(b'}')?;
                Php::Object { class, props: Vec::new() }
            }
            b'r' | b'R' => {
                self.until(b';')?;
                Php::Ref
            }
            b'E' => {
                let name = String::from_utf8_lossy(self.quoted()?).into_owned();
                self.byte(b';')?;
                Php::Enum(name)
            }
            _ => return None,
        })
    }
}

pub fn unserialize(bytes: &[u8]) -> Option<Php> {
    Reader { bytes, at: 0 }.value(0)
}

impl Php {
    fn text(&self) -> Option<String> {
        match self {
            Php::Str(s) => Some(String::from_utf8_lossy(s).into_owned()),
            Php::Int(i) => Some(i.to_string()),
            _ => None,
        }
    }

    fn int(&self) -> Option<i64> {
        match self {
            Php::Int(i) => Some(*i),
            Php::Str(s) => std::str::from_utf8(s).ok()?.parse().ok(),
            _ => None,
        }
    }

    /// A property by name, private and protected ones included ("\0Class\0name").
    fn prop(&self, name: &str) -> Option<&Php> {
        let Php::Object { props, .. } = self else { return None };
        props.iter().find_map(|(k, v)| {
            let Php::Str(k) = k else { return None };
            let bare = k.rsplit(|&b| b == 0).next().unwrap_or(k);
            (bare == name.as_bytes()).then_some(v)
        })
    }

    /// An array entry by key.
    fn get(&self, key: &str) -> Option<&Php> {
        let Php::Array(items) = self else { return None };
        items.iter().find(|(k, _)| k.text().as_deref() == Some(key)).map(|(_, v)| v)
    }
}

// ---------------------------------------------------------------------------------------
// VarDumper's Data, as a tree.

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    /// string, int, float, bool, null, array, object, resource, enum, more.
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Items in an array or object, string length.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<i64>,
    /// Items or characters VarDumper left out.
    #[serde(skip_serializing_if = "is_zero")]
    pub cut: i64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Child>,
}

fn is_zero(n: &i64) -> bool {
    *n == 0
}

#[derive(Debug, Clone, Serialize)]
pub struct Child {
    pub key: String,
    pub node: Node,
}

fn leaf(kind: &'static str, value: Option<String>) -> Node {
    Node { kind, class: None, value, count: None, cut: 0, children: Vec::new() }
}

struct Render<'a> {
    /// Data's table: position → that position's items.
    table: &'a [(Php, Php)],
    nodes: usize,
}

impl Render<'_> {
    fn position(&self, at: i64) -> Option<&Vec<(Php, Php)>> {
        self.table.iter().find(|(k, _)| k.int() == Some(at)).and_then(|(_, v)| if let Php::Array(items) = v { Some(items) } else { None })
    }

    fn item(&mut self, item: &Php, depth: usize) -> Node {
        self.nodes += 1;
        match item {
            Php::Null => leaf("null", None),
            Php::Bool(b) => leaf("bool", Some(b.to_string())),
            Php::Int(i) => leaf("int", Some(i.to_string())),
            Php::Float(f) => leaf("float", Some(f.to_string())),
            Php::Str(s) => {
                let text = String::from_utf8_lossy(s).into_owned();
                Node { count: Some(text.chars().count() as i64), ..leaf("string", Some(text)) }
            }
            // VarCloner keeps a non-empty array as one entry, [class => position]: class 1
            // for associative, 2 for indexed, and its items at that position of the table.
            Php::Array(items) => match items.as_slice() {
                [(Php::Int(class @ 1..=2), Php::Int(position))] => self.children(leaf("array", None), *class == 1, *position, false, depth),
                _ => Node { count: Some(0), ..leaf("array", None) },
            },
            Php::Ref => leaf("more", Some("(reference)".into())),
            Php::Enum(name) => leaf("enum", Some(name.clone())),
            Php::Object { class, .. } if class.ends_with("\\Data") => match data_root(item) {
                Some((table, root)) => Render { table, nodes: self.nodes }.item(root, depth),
                None => leaf("more", Some(class.clone())),
            },
            Php::Object { .. } => self.stub(item, depth),
        }
    }

    /// A VarDumper Stub; its properties left at their defaults aren't serialized.
    fn stub(&mut self, stub: &Php, depth: usize) -> Node {
        let ty = stub.prop("type").and_then(Php::int).unwrap_or(1);
        let class = stub.prop("class").and_then(Php::text).unwrap_or_default();
        let value = stub.prop("value");
        let cut = stub.prop("cut").and_then(Php::int).unwrap_or(0);
        let position = stub.prop("position").and_then(Php::int).unwrap_or(0);
        let mut node = match ty {
            // A hard reference: the value it points at.
            1 => return value.map_or_else(|| leaf("null", None), |v| self.item(v, depth)),
            2 => {
                let text = value.and_then(Php::text).unwrap_or_default();
                Node { count: Some(text.chars().count() as i64 + cut), cut, ..leaf("string", Some(text)) }
            }
            3 => Node { count: value.and_then(Php::int), cut, ..leaf("array", None) },
            4 => Node { class: Some(class), cut, ..leaf("object", None) },
            5 => Node { class: Some(class), cut, ..leaf("resource", None) },
            _ => leaf("more", value.and_then(Php::text).or(Some(class))),
        };
        if position > 0 && matches!(ty, 3..=5) {
            let object = ty != 3;
            node = self.children(node, true, position, object, depth);
        }
        node
    }

    /// Fills in the items at `position` of the table.
    fn children(&mut self, mut node: Node, keyed: bool, position: i64, object: bool, depth: usize) -> Node {
        let items = self.position(position).cloned().unwrap_or_default();
        if node.kind == "array" && node.count.is_none() {
            node.count = Some(items.len() as i64 + node.cut);
        }
        if depth >= MAX_DEPTH || self.nodes >= MAX_NODES {
            node.cut = node.cut.max(items.len() as i64);
            return node;
        }
        for (k, v) in &items {
            if self.nodes >= MAX_NODES {
                node.cut += 1;
                continue;
            }
            // Indexed arrays show their positions as plain numbers, like CliDumper.
            let key = if keyed || object { key_label(k, object) } else { k.text().unwrap_or_default() };
            let child = self.item(v, depth + 1);
            node.children.push(Child { key, node: child });
        }
        node
    }
}

/// An object's property key as CliDumper writes it: +public, #protected, -private, ~virtual.
fn key_label(key: &Php, object: bool) -> String {
    let text = key.text().unwrap_or_default();
    if !object {
        return text;
    }
    match text.strip_prefix('\0') {
        Some(rest) => match rest.split_once('\0') {
            Some(("*", name)) => format!("#{name}"),
            Some(("~", name)) => format!("~{name}"),
            Some(("+", name)) => format!("+{name}"),
            Some((_, name)) => format!("-{name}"),
            None => text,
        },
        None => format!("+{text}"),
    }
}

/// A Data object's table and the item its cursor points at.
fn data_root(data: &Php) -> Option<(&[(Php, Php)], &Php)> {
    let Php::Array(table) = data.prop("data")? else { return None };
    let position = data.prop("position").and_then(Php::int).unwrap_or(0);
    let key = data.prop("key").cloned().unwrap_or(Php::Int(0));
    let items = table.iter().find(|(k, _)| k.int() == Some(position)).map(|(_, v)| v)?;
    let Php::Array(items) = items else { return None };
    let root = items.iter().find(|(k, _)| k.text() == key.text()).map(|(_, v)| v).or_else(|| items.first().map(|(_, v)| v))?;
    Some((table.as_slice(), root))
}

pub fn render(data: &Php) -> Option<Node> {
    let (table, root) = data_root(data)?;
    Some(Render { table, nodes: 0 }.item(root, 0))
}

/// One line for the log: `array:3`, `"text"`, `App\Models\User`, `42`.
pub fn summary(node: &Node) -> String {
    match node.kind {
        "string" => {
            let v = node.value.as_deref().unwrap_or_default();
            let short: String = v.chars().take(120).collect();
            format!("\"{short}{}\"", if v.chars().count() > 120 || node.cut > 0 { "…" } else { "" })
        }
        "array" => format!("array:{}", node.count.unwrap_or(node.children.len() as i64)),
        "object" | "resource" => node.class.clone().unwrap_or_default(),
        _ => node.value.clone().unwrap_or_else(|| node.kind.to_string()),
    }
}

// ---------------------------------------------------------------------------------------
// The listener.

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dump {
    pub id: u64,
    pub time: String,
    /// The project whose code dumped, from the file's path.
    pub project: Option<String>,
    pub file: Option<String>,
    pub line: Option<i64>,
    /// "GET /users" for a request, the command line for a console command.
    pub origin: Option<String>,
    pub tree: Node,
}

#[derive(Default)]
pub struct DumpStore {
    list: Mutex<(u64, VecDeque<Dump>)>,
}

impl DumpStore {
    pub fn all(&self) -> Vec<Dump> {
        self.list.lock().unwrap().1.iter().cloned().collect()
    }

    pub fn clear(&self) {
        self.list.lock().unwrap().1.clear();
    }

    fn add(&self, mut dump: Dump) -> Dump {
        let mut guard = self.list.lock().unwrap();
        guard.0 += 1;
        dump.id = guard.0;
        guard.1.push_back(dump.clone());
        while guard.1.len() > KEEP {
            guard.1.pop_front();
        }
        dump
    }
}

/// Decodes one line ServerDumper sent.
pub fn decode(line: &str, www: &Path) -> Option<Dump> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(line.trim()).ok()?;
    let payload = unserialize(&bytes)?;
    let Php::Array(parts) = &payload else { return None };
    let data = &parts.first()?.1;
    let context = parts.get(1).map(|(_, v)| v);
    let tree = render(data)?;
    let source = context.and_then(|c| c.get("source"));
    let file = source.and_then(|s| s.get("file")).and_then(Php::text);
    let line = source.and_then(|s| s.get("line")).and_then(Php::int);
    let origin = context.and_then(|c| {
        let request = c.get("request");
        let method = request.and_then(|r| r.get("method")).and_then(Php::text);
        let uri = request.and_then(|r| r.get("uri")).and_then(Php::text);
        match (method, uri) {
            (Some(m), Some(u)) => Some(format!("{m} {u}")),
            _ => c.get("cli").and_then(|cli| cli.get("command_line")).and_then(Php::text),
        }
    });
    let project = file.as_deref().and_then(|f| project_of(Path::new(f), www));
    Some(Dump { id: 0, time: crate::logs::stamp(), project, file, line, origin, tree })
}

/// The folder in www/ a file belongs to.
fn project_of(file: &Path, www: &Path) -> Option<String> {
    let norm = |p: &Path| p.to_string_lossy().replace('\\', "/").to_lowercase();
    let (file_s, www_s) = (norm(file), norm(www));
    let rest = file_s.strip_prefix(&format!("{}/", www_s.trim_end_matches('/')))?;
    let first = rest.split('/').next()?;
    // Keep the folder's own spelling (Windows paths compare case-insensitively).
    let original = file.to_string_lossy().replace('\\', "/");
    let start = original.len() - rest.len();
    Some(original[start..start + first.len()].to_string())
}

/// The prepend file that turns on VarDumper's server mode for web requests.
pub fn prepend_script(port: u16) -> String {
    format!(
        "<?php\n// Written by LampRabit: sends dump() and dd() to LampRabit's Logs (Logs → Dumps).\n// Web requests only, so console commands (tinker) keep printing in the terminal; a\n// project that sets VAR_DUMPER_FORMAT itself keeps its own choice.\nif (PHP_SAPI !== 'cli' && PHP_SAPI !== 'phpdbg' && !isset($_SERVER['VAR_DUMPER_FORMAT'])) {{\n    $_SERVER['VAR_DUMPER_FORMAT'] = 'server';\n    $_SERVER['VAR_DUMPER_SERVER'] = '127.0.0.1:{port}';\n}}\n"
    )
}

/// Listens for dumps on 127.0.0.1:`port` until the task is aborted.
pub async fn serve(app: AppHandle, port: u16, www: PathBuf) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    loop {
        let (socket, _) = listener.accept().await?;
        let (app, www) = (app.clone(), www.clone());
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(socket).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Some(dump) = decode(&line, &www) else { continue };
                let dump = app.state::<DumpStore>().add(dump);
                let place = match (&dump.file, dump.line) {
                    (Some(f), Some(l)) => format!(" · {}:{l}", Path::new(f).file_name().map_or(f.clone(), |n| n.to_string_lossy().into_owned())),
                    _ => String::new(),
                };
                let text = format!("[{}] info: dump #{} {}{place}", dump.time, dump.id, summary(&dump.tree));
                crate::logs::push_dump(&app, dump.project.clone(), text);
                let _ = app.emit("dump", dump);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ser_str(s: &str) -> String {
        format!("s:{}:\"{s}\";", s.len())
    }

    #[test]
    fn reads_php_serialize() {
        let v = unserialize(b"a:3:{i:0;s:3:\"abc\";s:1:\"k\";d:1.5;i:2;b:1;}").unwrap();
        let Php::Array(items) = v else { panic!() };
        assert_eq!(items[0].1, Php::Str(b"abc".to_vec()));
        assert_eq!(items[1].1, Php::Float(1.5));
        assert_eq!(items[2].1, Php::Bool(true));
        assert!(unserialize(b"O:8:\"stdClass\":1:{s:1:\"a\";N;}").is_some());
        assert!(unserialize(b"s:10:\"short\";").is_none(), "a wrong length is refused");
    }

    #[test]
    fn renders_a_dumped_array() {
        // dump(['name' => 'Ada', 'tags' => [1, 2]]) as VarCloner stores it (checked against
        // real VarDumper output): arrays are [class => position], 1 associative and 2 indexed;
        // the root's items are at position 1, the inner array's at 2.
        let table = format!(
            "a:3:{{i:0;a:1:{{i:0;a:1:{{i:1;i:1;}}}}i:1;a:2:{{{}{}{}a:1:{{i:2;i:2;}}}}i:2;a:2:{{i:0;i:1;i:1;i:2;}}}}",
            ser_str("name"),
            ser_str("Ada"),
            ser_str("tags"),
        );
        let data = format!("O:39:\"Symfony\\Component\\VarDumper\\Cloner\\Data\":1:{{s:45:\"\0Symfony\\Component\\VarDumper\\Cloner\\Data\0data\";{table}}}");
        let php = unserialize(data.as_bytes()).expect("parses");
        let tree = render(&php).expect("renders");
        assert_eq!(tree.kind, "array");
        assert_eq!(tree.children[0].key, "name");
        assert_eq!(tree.children[0].node.value.as_deref(), Some("Ada"));
        assert_eq!(tree.children[1].node.children.len(), 2);
        assert_eq!(summary(&tree), "array:2");
    }

    /// A payload from real VarDumper, when LAMPRABIT_DUMP_SAMPLE names a file holding one
    /// (base64 of serialize([$data, $context]), as ServerDumper sends it).
    #[test]
    fn decodes_a_real_payload() {
        let Ok(path) = std::env::var("LAMPRABIT_DUMP_SAMPLE") else { return };
        let line = std::fs::read_to_string(path).unwrap();
        let dump = decode(&line, Path::new("C:/lampRabit/www")).expect("decodes");
        println!("{}", serde_json::to_string_pretty(&dump).unwrap());
        assert_eq!(dump.project.as_deref(), Some("dummy-project"));
        assert_eq!(dump.line, Some(12));
        let user = &dump.tree.children.iter().find(|c| c.key == "user").unwrap().node;
        assert_eq!(user.class.as_deref(), Some("Person"));
        assert!(user.children.iter().any(|c| c.key == "#age") && user.children.iter().any(|c| c.key == "-secret"));
    }

    /// A line captured from ServerDumper itself (LAMPRABIT_DUMP_WIRE), for
    /// dump(['order' => new Order(), 'total' => 19.5]) with its own context.
    #[test]
    fn decodes_what_server_dumper_sends() {
        let Ok(path) = std::env::var("LAMPRABIT_DUMP_WIRE") else { return };
        let dump = decode(&std::fs::read_to_string(path).unwrap(), Path::new("C:/lampRabit/www")).expect("decodes");
        println!("{} · {:?}:{:?}", summary(&dump.tree), dump.file, dump.line);
        let order = &dump.tree.children.iter().find(|c| c.key == "order").unwrap().node;
        assert_eq!(order.class.as_deref(), Some("Order"));
        assert!(order.children.iter().any(|c| c.key == "#items"));
        assert!(dump.line.is_some(), "VarDumper's source context names the line");
    }

    #[test]
    fn names_object_keys_like_clidumper() {
        assert_eq!(key_label(&Php::Str(b"\0*\0items".to_vec()), true), "#items");
        assert_eq!(key_label(&Php::Str(b"\0App\\User\0secret".to_vec()), true), "-secret");
        assert_eq!(key_label(&Php::Str(b"name".to_vec()), true), "+name");
    }

    #[test]
    fn finds_the_project_of_a_file() {
        let www = Path::new("C:/lampRabit/www");
        assert_eq!(project_of(Path::new("C:\\lampRabit\\www\\Shop\\app\\Http\\x.php"), www).as_deref(), Some("Shop"));
        assert_eq!(project_of(Path::new("/elsewhere/x.php"), www), None);
    }
}
