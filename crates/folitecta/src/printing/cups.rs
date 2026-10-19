//! The local print server of Linux and macOS (CUPS), reached over IPP on its
//! local socket or on `localhost:631`. The print file is streamed from memory
//! (Folitecta writes no file; the print server keeps the job in its spool as
//! usual). No program is started and no other computer is contacted: a print
//! server set to another computer is refused, so its printers are reached
//! through the system dialog.

use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::ipp::{self, Attr, Group, Message};
use super::{JobOutcome, JobProgress, JobTicket, PrintBackend, PrintError, PrinterCaps, PrinterInfo};

/// Where the print server listens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    Socket(PathBuf),
    Tcp(String, u16),
}

const SOCKETS: &[&str] = &[
    "/run/cups/cups.sock",
    "/var/run/cups/cups.sock",
    "/private/var/run/cupsd",
    "/var/run/cupsd",
];

const TIMEOUT: Duration = Duration::from_secs(60);
const CHUNK: usize = 256 * 1024;

/// The value of the `ServerName` line of a client configuration.
fn server_name(conf: &str) -> Option<String> {
    conf.lines().find_map(|l| {
        let l = l.trim();
        if l.starts_with('#') {
            return None;
        }
        let mut parts = l.split_whitespace();
        (parts.next()?.eq_ignore_ascii_case("ServerName"))
            .then(|| parts.next().map(str::to_string))
            .flatten()
    })
}

/// The server a `ServerName` or `CUPS_SERVER` value names: a socket path, or
/// this computer with an optional port; any other computer is refused.
pub fn endpoint_of(value: &str) -> Result<Endpoint, PrintError> {
    let v = value.trim();
    if v.starts_with('/') {
        return Ok(Endpoint::Socket(PathBuf::from(v)));
    }
    let (host, port) = if let Some(rest) = v.strip_prefix('[') {
        let (h, after) = rest.split_once(']').unwrap_or((rest, ""));
        (
            h.to_string(),
            after.strip_prefix(':').and_then(|p| p.parse().ok()),
        )
    } else if v.matches(':').count() == 1 {
        let (h, p) = v.split_once(':').unwrap_or((v, ""));
        (h.to_string(), p.parse().ok())
    } else {
        (v.to_string(), None)
    };
    let port = port.unwrap_or(631);
    match host.to_ascii_lowercase().as_str() {
        "localhost" | "127.0.0.1" => Ok(Endpoint::Tcp("127.0.0.1".into(), port)),
        "::1" => Ok(Endpoint::Tcp("::1".into(), port)),
        _ => Err(PrintError::RemoteServer(host)),
    }
}

/// The print server of this computer as configured: `CUPS_SERVER`, then the
/// `ServerName` of the user's and the system's client configuration, then
/// the usual sockets. `None`: try `localhost:631`.
pub fn discover(
    env: &dyn Fn(&str) -> Option<String>,
    read: &dyn Fn(&Path) -> Option<String>,
    exists: &dyn Fn(&Path) -> bool,
) -> Result<Option<Endpoint>, PrintError> {
    if let Some(v) = env("CUPS_SERVER") {
        return endpoint_of(&v).map(Some);
    }
    let mut confs = Vec::new();
    if let Some(home) = env("HOME") {
        confs.push(Path::new(&home).join(".cups/client.conf"));
    }
    confs.push(PathBuf::from("/etc/cups/client.conf"));
    for conf in confs {
        if let Some(name) = read(&conf).as_deref().and_then(server_name) {
            return endpoint_of(&name).map(Some);
        }
    }
    Ok(SOCKETS
        .iter()
        .map(Path::new)
        .find(|p| exists(p))
        .map(|p| Endpoint::Socket(p.to_path_buf())))
}

/// The user's own default printer: `PRINTER` or `LPDEST`, else the `Default`
/// of the user's printer options.
pub fn user_default(
    env: &dyn Fn(&str) -> Option<String>,
    read: &dyn Fn(&Path) -> Option<String>,
) -> Option<String> {
    let strip = |s: &str| s.split('/').next().unwrap_or(s).trim().to_string();
    for key in ["PRINTER", "LPDEST"] {
        if let Some(v) = env(key) {
            return Some(strip(&v));
        }
    }
    let home = env("HOME")?;
    let options = read(&Path::new(&home).join(".cups/lpoptions"))?;
    options.lines().find_map(|l| {
        let mut parts = l.split_whitespace();
        (parts.next()? == "Default")
            .then(|| parts.next().map(strip))
            .flatten()
    })
}

fn real_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

fn real_read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// The print server of this computer (never reached by the tests).
pub fn server() -> Result<Endpoint, PrintError> {
    if cfg!(test) {
        return Err(PrintError::NoServer);
    }
    if let Some(e) = discover(&real_env, &real_read, &|p| p.exists())? {
        return Ok(e);
    }
    let addr = SocketAddr::from(([127, 0, 0, 1], 631));
    TcpStream::connect_timeout(&addr, Duration::from_secs(1))
        .map(|_| Endpoint::Tcp("127.0.0.1".into(), 631))
        .map_err(|_| PrintError::NoServer)
}

/// The user name told to the print server.
fn user_name() -> String {
    real_env("USER")
        .or_else(|| real_env("LOGNAME"))
        .unwrap_or_else(|| "folitecta".into())
}

enum Conn {
    Unix(UnixStream),
    Tcp(TcpStream),
}

impl Conn {
    fn open(e: &Endpoint) -> Result<Conn, PrintError> {
        let io = |err: std::io::Error| PrintError::Io(err.to_string());
        let conn = match e {
            Endpoint::Socket(path) => {
                let s = UnixStream::connect(path).map_err(|_| PrintError::NoServer)?;
                s.set_read_timeout(Some(TIMEOUT)).map_err(io)?;
                s.set_write_timeout(Some(TIMEOUT)).map_err(io)?;
                Conn::Unix(s)
            }
            Endpoint::Tcp(host, port) => {
                let addr = (host.as_str(), *port)
                    .to_socket_addrs()
                    .map_err(io)?
                    .next()
                    .ok_or(PrintError::NoServer)?;
                let s = TcpStream::connect_timeout(&addr, Duration::from_secs(2))
                    .map_err(|_| PrintError::NoServer)?;
                s.set_read_timeout(Some(TIMEOUT)).map_err(io)?;
                s.set_write_timeout(Some(TIMEOUT)).map_err(io)?;
                Conn::Tcp(s)
            }
        };
        Ok(conn)
    }

    fn shutdown(&self) {
        let _ = match self {
            Conn::Unix(s) => s.shutdown(Shutdown::Both),
            Conn::Tcp(s) => s.shutdown(Shutdown::Both),
        };
    }
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Conn::Unix(s) => s.read(buf),
            Conn::Tcp(s) => s.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Conn::Unix(s) => s.write(buf),
            Conn::Tcp(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Conn::Unix(s) => s.flush(),
            Conn::Tcp(s) => s.flush(),
        }
    }
}

/// Requests to one print server, with the credentials it asked for.
struct Session {
    endpoint: Endpoint,
    user: String,
    auth: Option<String>,
    next_id: u32,
}

impl Session {
    fn new(endpoint: Endpoint) -> Self {
        Session {
            endpoint,
            user: user_name(),
            auth: None,
            next_id: 1,
        }
    }

    /// POST an IPP request (and a document) to `path`; the answer.
    fn request(
        &mut self,
        path: &str,
        op: u16,
        groups: &[Group],
        doc: Option<&[u8]>,
        progress: &dyn Fn(JobProgress),
        cancel: &AtomicBool,
    ) -> Result<Message, PrintError> {
        let id = self.next_id;
        self.next_id += 1;
        let body = ipp::encode(op, id, groups);
        for attempt in 0..2 {
            let mut c = Conn::open(&self.endpoint)?;
            let total = body.len() + doc.map_or(0, <[u8]>::len);
            let mut head = format!(
                "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/ipp\r\nContent-Length: {total}\r\nUser-Agent: Folitecta/{}\r\nConnection: close\r\n",
                folitecta_core::VERSION
            );
            if let Some(a) = &self.auth {
                head.push_str(&format!("Authorization: {a}\r\n"));
            }
            head.push_str("\r\n");
            let failed = |e: std::io::Error| {
                if cancel.load(Ordering::Relaxed) {
                    PrintError::Cancelled
                } else {
                    PrintError::Io(e.to_string())
                }
            };
            c.write_all(head.as_bytes()).map_err(failed)?;
            c.write_all(&body).map_err(failed)?;
            if let Some(doc) = doc {
                let mut sent = 0u64;
                for chunk in doc.chunks(CHUNK) {
                    if cancel.load(Ordering::Relaxed) {
                        c.shutdown();
                        return Err(PrintError::Cancelled);
                    }
                    c.write_all(chunk).map_err(failed)?;
                    sent += chunk.len() as u64;
                    progress(JobProgress::Sending {
                        sent,
                        total: doc.len() as u64,
                    });
                }
            }
            c.flush().map_err(failed)?;
            if cancel.load(Ordering::Relaxed) {
                c.shutdown();
                return Err(PrintError::Cancelled);
            }
            let answer = crate::http::read_answer(&mut c).map_err(PrintError::Io)?;
            match answer.status {
                200 => return ipp::decode(&answer.body).map_err(PrintError::Io),
                401 if attempt == 0
                    && matches!(self.endpoint, Endpoint::Socket(_))
                    && answer
                        .header("WWW-Authenticate")
                        .is_some_and(|h| h.contains("PeerCred")) =>
                {
                    // The server checks who is on the other end of its socket.
                    self.auth = Some(format!("PeerCred {}", self.user));
                }
                401 | 403 => return Err(PrintError::Auth),
                status => {
                    return Err(PrintError::Server {
                        status,
                        message: answer.reason,
                    });
                }
            }
        }
        Err(PrintError::Auth)
    }

    fn simple(&mut self, path: &str, op: u16, groups: &[Group]) -> Result<Message, PrintError> {
        let m = self.request(path, op, groups, None, &|_| {}, &AtomicBool::new(false))?;
        match ipp::status_error(m.code, &m.status_message()) {
            Some(e) => Err(e),
            None => Ok(m),
        }
    }

    fn operation(&self, uri: Option<&str>) -> Vec<Attr> {
        ipp::operation_attrs(uri, &self.user, "en")
    }
}

/// The path of a queue on the server.
fn queue_path(name: &str) -> String {
    ipp::printer_uri(name)
        .trim_start_matches("ipp://localhost")
        .to_string()
}

/// The printers of the local print server.
#[derive(Debug, Default)]
pub struct Cups {
    /// A fixed server (the tests); otherwise found on each use.
    endpoint: Option<Endpoint>,
}

impl Cups {
    /// The print server at `endpoint` (the tests' own).
    #[cfg(test)]
    pub fn at(endpoint: Endpoint) -> Self {
        Cups {
            endpoint: Some(endpoint),
        }
    }

    fn session(&self) -> Result<Session, PrintError> {
        let e = match &self.endpoint {
            Some(e) => e.clone(),
            None => server()?,
        };
        Ok(Session::new(e))
    }

    fn caps_with(&self, s: &mut Session, printer: &str) -> Result<PrinterCaps, PrintError> {
        let uri = ipp::printer_uri(printer);
        let mut op = s.operation(Some(&uri));
        op.push(ipp::requested(ipp::CAPS_ATTRS));
        let m = s.simple(
            &queue_path(printer),
            ipp::GET_PRINTER_ATTRIBUTES,
            &[Group {
                tag: ipp::OPERATION,
                attrs: op,
            }],
        )?;
        Ok(ipp::caps_from(&m))
    }

    fn job_groups(s: &Session, t: &JobTicket, caps: &PrinterCaps, skip: &[String]) -> Vec<Group> {
        let uri = ipp::printer_uri(&t.printer);
        let mut op = s.operation(Some(&uri));
        op.push(Attr::text(ipp::NAME, "job-name", &ipp::name_value(&t.job_name)));
        op.push(Attr::text(ipp::MIME, "document-format", "application/pdf"));
        vec![
            Group {
                tag: ipp::OPERATION,
                attrs: op,
            },
            Group {
                tag: ipp::JOB,
                attrs: ipp::job_attributes(t, caps, skip),
            },
        ]
    }
}

impl PrintBackend for Cups {
    fn printers(&self) -> Result<Vec<PrinterInfo>, PrintError> {
        let mut s = self.session()?;
        let mut op = s.operation(None);
        op.push(ipp::requested(ipp::PRINTER_LIST_ATTRS));
        let list = s.request(
            "/",
            ipp::CUPS_GET_PRINTERS,
            &[Group {
                tag: ipp::OPERATION,
                attrs: op,
            }],
            None,
            &|_| {},
            &AtomicBool::new(false),
        )?;
        // A server without queues answers "not found" ("No destinations added").
        if list.code == 0x0406 {
            return Ok(Vec::new());
        }
        if let Some(e) = ipp::status_error(list.code, &list.status_message()) {
            return Err(e);
        }
        let server_default = {
            let mut op = s.operation(None);
            op.push(ipp::requested(&["printer-name"]));
            s.simple(
                "/",
                ipp::CUPS_GET_DEFAULT,
                &[Group {
                    tag: ipp::OPERATION,
                    attrs: op,
                }],
            )
            .ok()
            .and_then(|m| {
                m.attr(ipp::PRINTER, "printer-name")
                    .and_then(Attr::first_str)
                    .map(str::to_string)
            })
        };
        let default = user_default(&real_env, &real_read).or(server_default);
        Ok(ipp::printers_from(&list, default.as_deref()))
    }

    fn capabilities(&self, printer: &str) -> Result<PrinterCaps, PrintError> {
        let mut s = self.session()?;
        self.caps_with(&mut s, printer)
    }

    fn submit(
        &self,
        t: &JobTicket,
        pdf: Arc<Vec<u8>>,
        progress: &dyn Fn(JobProgress),
        cancel: &AtomicBool,
    ) -> Result<JobOutcome, PrintError> {
        let mut s = self.session()?;
        let caps = self.caps_with(&mut s, &t.printer)?;
        if !caps.accepts_pdf {
            return Err(PrintError::FormatNotSupported);
        }
        let path = queue_path(&t.printer);
        // A check without the document: credentials, a printer that does not
        // accept jobs, settings it does not support (left out of the job).
        let check = s.request(
            &path,
            ipp::VALIDATE_JOB,
            &Self::job_groups(&s, t, &caps, &[]),
            None,
            &|_| {},
            cancel,
        )?;
        let mut ignored = check.unsupported();
        if check.code != 0x040B {
            if let Some(e) = ipp::status_error(check.code, &check.status_message()) {
                return Err(e);
            }
        }
        for attempt in 0..2 {
            let m = s.request(
                &path,
                ipp::PRINT_JOB,
                &Self::job_groups(&s, t, &caps, &ignored),
                Some(pdf.as_slice()),
                progress,
                cancel,
            )?;
            if m.code == 0x040B && attempt == 0 {
                let more = m.unsupported();
                if !more.is_empty() && more.iter().any(|n| !ignored.contains(n)) {
                    ignored.extend(more);
                    continue;
                }
            }
            if let Some(e) = ipp::status_error(m.code, &m.status_message()) {
                return Err(e);
            }
            ignored.extend(m.unsupported());
            ignored.sort();
            ignored.dedup();
            let job_id = m
                .attr(ipp::JOB, "job-id")
                .and_then(Attr::first_int)
                .map(|i| i.max(0) as u32);
            return Ok(JobOutcome { job_id, ignored });
        }
        Err(PrintError::Server {
            status: 0x040B,
            message: "attributes or values not supported".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printing::ipp::{Value, encode};
    use crate::printing::{Media, PrintBackend};
    use folitecta_core::print::Sides;
    use std::collections::HashMap;
    use std::os::unix::net::UnixListener;
    use std::sync::Mutex;

    #[test]
    fn the_server_is_found_as_configured() {
        let files: HashMap<PathBuf, String> = HashMap::from([
            (
                PathBuf::from("/home/u/.cups/client.conf"),
                "# mine\nServerName /tmp/my.sock\n".into(),
            ),
            (
                PathBuf::from("/etc/cups/client.conf"),
                "ServerName print.example.org:631\n".into(),
            ),
        ]);
        let read = |p: &Path| files.get(p).cloned();
        let none = |_: &Path| false;
        // CUPS_SERVER first.
        let env = |k: &str| match k {
            "CUPS_SERVER" => Some("localhost:8631".to_string()),
            "HOME" => Some("/home/u".into()),
            _ => None,
        };
        assert_eq!(
            discover(&env, &read, &none),
            Ok(Some(Endpoint::Tcp("127.0.0.1".into(), 8631)))
        );
        // The user's configuration before the system's.
        let env = |k: &str| (k == "HOME").then(|| "/home/u".to_string());
        assert_eq!(
            discover(&env, &read, &none),
            Ok(Some(Endpoint::Socket("/tmp/my.sock".into())))
        );
        // The system's names another computer: refused.
        let env = |_: &str| None;
        assert_eq!(
            discover(&env, &read, &none),
            Err(PrintError::RemoteServer("print.example.org".into()))
        );
        // No configuration: the first socket that exists, else localhost.
        let nothing = |_: &Path| None;
        let exists = |p: &Path| p == Path::new("/var/run/cups/cups.sock");
        assert_eq!(
            discover(&env, &nothing, &exists),
            Ok(Some(Endpoint::Socket("/var/run/cups/cups.sock".into())))
        );
        assert_eq!(discover(&env, &nothing, &none), Ok(None));
        assert_eq!(endpoint_of("[::1]:700"), Ok(Endpoint::Tcp("::1".into(), 700)));
        assert_eq!(
            endpoint_of("127.0.0.1"),
            Ok(Endpoint::Tcp("127.0.0.1".into(), 631))
        );
        assert!(matches!(
            endpoint_of("10.0.0.2"),
            Err(PrintError::RemoteServer(_))
        ));
        // The tests never reach the print server of this computer.
        assert_eq!(server(), Err(PrintError::NoServer));
    }

    #[test]
    fn the_users_default_printer() {
        let files: HashMap<PathBuf, String> = HashMap::from([(
            PathBuf::from("/home/u/.cups/lpoptions"),
            "Dest Other\nDefault Laser/draft sides=two-sided-long-edge\n".into(),
        )]);
        let read = |p: &Path| files.get(p).cloned();
        let env = |k: &str| (k == "HOME").then(|| "/home/u".to_string());
        assert_eq!(user_default(&env, &read).as_deref(), Some("Laser"));
        let env = |k: &str| match k {
            "PRINTER" => Some("Photo".to_string()),
            "HOME" => Some("/home/u".into()),
            _ => None,
        };
        assert_eq!(user_default(&env, &read).as_deref(), Some("Photo"));
        assert_eq!(user_default(&|_| None, &read), None);
    }

    /// One request received by the fake server.
    #[derive(Debug, Clone)]
    struct Received {
        head: String,
        op: u16,
        message: Message,
        doc: Vec<u8>,
        complete: bool,
    }

    fn read_request(s: &mut dyn Read) -> Option<Received> {
        let mut data = Vec::new();
        let mut buf = [0u8; 64 * 1024];
        let (head_end, len) = loop {
            let n = s.read(&mut buf).ok()?;
            if n == 0 {
                return None;
            }
            data.extend_from_slice(&buf[..n]);
            if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&data[..end]).to_ascii_lowercase();
                let len = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                break (end, len);
            }
        };
        let mut complete = true;
        while data.len() < head_end + 4 + len {
            match s.read(&mut buf) {
                Ok(0) | Err(_) => {
                    complete = false;
                    break;
                }
                Ok(n) => data.extend_from_slice(&buf[..n]),
            }
        }
        let head = String::from_utf8_lossy(&data[..head_end]).into_owned();
        let body = &data[head_end + 4..];
        let message = ipp::decode(body).ok()?;
        let ipp_len = ipp::encode(message.code, message.request_id, &message.groups).len();
        Some(Received {
            head,
            op: message.code,
            doc: body.get(ipp_len..).unwrap_or_default().to_vec(),
            message,
            complete,
        })
    }

    fn answer(status: u16, groups: Vec<Group>) -> Vec<u8> {
        let mut all = vec![Group {
            tag: ipp::OPERATION,
            attrs: vec![
                Attr::text(ipp::CHARSET, "attributes-charset", "utf-8"),
                Attr::text(ipp::LANGUAGE, "attributes-natural-language", "en"),
            ],
        }];
        all.extend(groups);
        let body = encode(status, 1, &all);
        let mut out = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/ipp\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .into_bytes();
        out.extend_from_slice(&body);
        out
    }

    /// The answer of a small print server with one printer, "Office".
    fn respond(r: &Received, need_auth: bool) -> Vec<u8> {
        if need_auth && !r.head.contains("Authorization: PeerCred ") {
            return b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: PeerCred\r\nContent-Length: 0\r\n\r\n"
                .to_vec();
        }
        let printer = |attrs: Vec<Attr>| Group {
            tag: ipp::PRINTER,
            attrs,
        };
        match r.op {
            ipp::CUPS_GET_PRINTERS => answer(
                0,
                vec![printer(vec![
                    Attr::text(ipp::NAME, "printer-name", "Office"),
                    Attr::int(ipp::ENUM, "printer-state", 3),
                ])],
            ),
            ipp::CUPS_GET_DEFAULT => answer(
                0,
                vec![printer(vec![Attr::text(ipp::NAME, "printer-name", "Office")])],
            ),
            ipp::GET_PRINTER_ATTRIBUTES => answer(
                0,
                vec![printer(vec![
                    Attr::new(
                        ipp::KEYWORD,
                        "media-supported",
                        vec![Value::Str("iso_a4_210x297mm".into())],
                    ),
                    Attr::new(
                        ipp::KEYWORD,
                        "sides-supported",
                        vec![
                            Value::Str("one-sided".into()),
                            Value::Str("two-sided-short-edge".into()),
                        ],
                    ),
                    Attr::new(ipp::RANGE, "copies-supported", vec![Value::Range(1, 99)]),
                    Attr::text(ipp::KEYWORD, "print-scaling-supported", "none"),
                    Attr::text(ipp::MIME, "document-format-supported", "application/pdf"),
                ])],
            ),
            ipp::VALIDATE_JOB => answer(
                0x0001,
                vec![Group {
                    tag: ipp::UNSUPPORTED,
                    attrs: vec![Attr::text(ipp::KEYWORD, "print-scaling", "none")],
                }],
            ),
            ipp::PRINT_JOB => answer(
                0,
                vec![Group {
                    tag: ipp::JOB,
                    attrs: vec![Attr::int(ipp::INTEGER, "job-id", 7)],
                }],
            ),
            _ => answer(0x0501, Vec::new()),
        }
    }

    /// A print server on a socket in a folder of its own, answering every
    /// connection; the requests it received.
    fn unix_server(name: &str, need_auth: bool) -> (Endpoint, Arc<Mutex<Vec<Received>>>) {
        unix_server_with(name, move |r| respond(r, need_auth))
    }

    /// A print server on a socket answering with `answer`.
    fn unix_server_with(
        name: &str,
        answer: impl Fn(&Received) -> Vec<u8> + Send + 'static,
    ) -> (Endpoint, Arc<Mutex<Vec<Received>>>) {
        let dir = std::env::temp_dir().join(format!("folitecta-ipp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cups.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        let seen = log.clone();
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                let Some(r) = read_request(&mut s) else { continue };
                let reply = answer(&r);
                seen.lock().unwrap().push(r.clone());
                if r.complete {
                    let _ = s.write_all(&reply);
                }
            }
        });
        (Endpoint::Socket(path), log)
    }

    fn ticket() -> JobTicket {
        JobTicket {
            printer: "Office".into(),
            job_name: "Report".into(),
            copies: 1,
            collate: true,
            sides: Sides::ShortEdge,
            media: Some(Media {
                key: "iso_a4_210x297mm".into(),
                ..Default::default()
            }),
            gray: false,
            dpi: 300,
            devmode: None,
        }
    }

    #[test]
    fn a_job_goes_to_the_print_server_on_its_socket() {
        let (endpoint, log) = unix_server("job", false);
        let cups = Cups::at(endpoint);
        let printers = cups.printers().unwrap();
        assert_eq!(printers.len(), 1);
        assert!(printers[0].is_default || std::env::var_os("PRINTER").is_some());
        let caps = cups.capabilities("Office").unwrap();
        assert!(caps.duplex && caps.scaling_none);
        let pdf = Arc::new(vec![b'x'; 600 * 1024]);
        let progress = Mutex::new(Vec::new());
        let out = cups
            .submit(
                &ticket(),
                pdf.clone(),
                &|p| progress.lock().unwrap().push(p),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(out.job_id, Some(7));
        assert_eq!(out.ignored, ["print-scaling"]);
        let log = log.lock().unwrap();
        let ops: Vec<u16> = log.iter().map(|r| r.op).collect();
        assert_eq!(
            ops,
            [
                ipp::CUPS_GET_PRINTERS,
                ipp::CUPS_GET_DEFAULT,
                ipp::GET_PRINTER_ATTRIBUTES,
                ipp::GET_PRINTER_ATTRIBUTES,
                ipp::VALIDATE_JOB,
                ipp::PRINT_JOB
            ]
        );
        let job = &log[5];
        assert!(
            job.head.starts_with("POST /printers/Office HTTP/1.1\r\n"),
            "{}",
            job.head
        );
        assert!(job.head.contains("Content-Type: application/ipp"));
        assert_eq!(job.doc, *pdf);
        let attrs = |name: &str| job.message.attr(ipp::JOB, name).cloned();
        assert_eq!(attrs("sides").unwrap().first_str(), Some("two-sided-short-edge"));
        assert_eq!(attrs("media").unwrap().first_str(), Some("iso_a4_210x297mm"));
        assert_eq!(attrs("orientation-requested").unwrap().first_int(), Some(3));
        assert!(attrs("print-scaling").is_none(), "left out after the check");
        assert!(attrs("copies").is_none());
        assert_eq!(
            job.message
                .attr(ipp::OPERATION, "printer-uri")
                .and_then(|a| a.first_str().map(str::to_string)),
            Some("ipp://localhost/printers/Office".into())
        );
        let sent = progress.lock().unwrap();
        assert_eq!(
            sent.last(),
            Some(&JobProgress::Sending {
                sent: pdf.len() as u64,
                total: pdf.len() as u64
            })
        );
        assert_eq!(sent.len(), 3);
    }

    #[test]
    fn the_server_that_asks_who_is_calling_gets_the_peer_credentials() {
        let (endpoint, log) = unix_server("auth", true);
        let cups = Cups::at(endpoint);
        let out = cups
            .submit(
                &ticket(),
                Arc::new(b"%PDF".to_vec()),
                &|_| {},
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(out.job_id, Some(7));
        let log = log.lock().unwrap();
        assert!(!log[0].head.contains("Authorization"));
        assert!(
            log[1]
                .head
                .contains(&format!("Authorization: PeerCred {}", user_name()))
        );
        assert!(log.last().unwrap().head.contains("Authorization: PeerCred "));
    }

    #[test]
    fn a_cancelled_job_stops_sending() {
        let (endpoint, log) = unix_server("cancel", false);
        let cups = Cups::at(endpoint);
        let cancel = AtomicBool::new(false);
        let pdf = Arc::new(vec![b'y'; 4 * CHUNK]);
        let err = cups
            .submit(
                &ticket(),
                pdf,
                &|p| {
                    if let JobProgress::Sending { sent, .. } = p {
                        if sent >= CHUNK as u64 {
                            cancel.store(true, Ordering::Relaxed);
                        }
                    }
                },
                &cancel,
            )
            .unwrap_err();
        assert_eq!(err, PrintError::Cancelled);
        // Give the server a moment to see the connection end.
        for _ in 0..50 {
            if log.lock().unwrap().iter().any(|r| r.op == ipp::PRINT_JOB) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let log = log.lock().unwrap();
        let job = log.iter().find(|r| r.op == ipp::PRINT_JOB).unwrap();
        assert!(!job.complete && job.doc.len() < 4 * CHUNK);
    }

    #[test]
    fn a_server_without_queues_has_no_printers() {
        // CUPS answers client-error-not-found when no queue was added.
        let (endpoint, _log) = unix_server_with("empty", |r| match r.op {
            ipp::CUPS_GET_PRINTERS | ipp::CUPS_GET_DEFAULT => {
                // The status message of CUPS.
                let body = encode(
                    0x0406,
                    1,
                    &[Group {
                        tag: ipp::OPERATION,
                        attrs: vec![
                            Attr::text(ipp::CHARSET, "attributes-charset", "utf-8"),
                            Attr::text(ipp::TEXT, "status-message", "No destinations added."),
                        ],
                    }],
                );
                let mut a = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/ipp\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                )
                .into_bytes();
                a.extend_from_slice(&body);
                a
            }
            _ => respond(r, false),
        });
        assert_eq!(Cups::at(endpoint).printers(), Ok(Vec::new()));
    }

    #[test]
    fn the_server_on_localhost_answers_too() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                if let Some(r) = read_request(&mut s) {
                    let _ = s.write_all(&respond(&r, false));
                }
            }
        });
        let cups = Cups::at(Endpoint::Tcp("127.0.0.1".into(), port));
        assert_eq!(cups.printers().unwrap()[0].name, "Office");
        // Peer credentials are only offered on the local socket.
        assert_eq!(
            Cups::at(Endpoint::Tcp("127.0.0.1".into(), 1)).printers(),
            Err(PrintError::NoServer)
        );
    }
}
