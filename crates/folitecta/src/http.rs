//! A small HTTP/1.1 client for the requests Folitecta sends on its own: time
//! stamps (a POST to the server the user set) and revocation data (an OCSP
//! POST or the GET of a revocation list, only when the user turned online
//! revocation checks on). Plain or over the system's TLS (native-tls),
//! following a few redirections, each request within a time limit.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// Time a connection or a read may wait.
const TIMEOUT: Duration = Duration::from_secs(20);
/// Time a whole request (connection, request, answer) may take.
const DEADLINE: Duration = Duration::from_secs(60);
/// Largest answer accepted for a POST (time stamps, OCSP answers).
const MAX_ANSWER: usize = 8 * 1024 * 1024;
/// Largest answer accepted for a GET (revocation lists).
const MAX_DOWNLOAD: usize = 32 * 1024 * 1024;
/// Longest line of the size of a chunk.
const MAX_CHUNK_LINE: usize = 1024;

/// An address split for a request.
#[derive(Debug, Clone, PartialEq)]
struct Url {
    tls: bool,
    host: String,
    port: u16,
    /// Path and query, starting with `/`.
    target: String,
}

fn parse_url(url: &str) -> Result<Url, String> {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    let (tls, rest) = if lower.starts_with("https://") {
        (true, &url[8..])
    } else if lower.starts_with("http://") {
        (false, &url[7..])
    } else {
        return Err("only http:// and https:// addresses are supported".into());
    };
    let (authority, target) = match rest.find(['/', '?', '#']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let target = target.split('#').next().unwrap_or("/");
    let target = if target.starts_with('/') {
        target.to_string()
    } else {
        format!("/{target}")
    };
    if authority.contains('@') {
        return Err("addresses with a user name are not supported".into());
    }
    let default_port = if tls { 443 } else { 80 };
    let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
        let (host, after) = v6.split_once(']').ok_or("the address is not valid")?;
        let port = match after.strip_prefix(':') {
            Some(p) => p.parse().map_err(|_| "the port of the address is not valid")?,
            None => default_port,
        };
        (host.to_string(), port)
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (
                h.to_string(),
                p.parse().map_err(|_| "the port of the address is not valid")?,
            ),
            None => (authority.to_string(), default_port),
        }
    };
    if host.is_empty() {
        return Err("the address has no server name".into());
    }
    Ok(Url {
        tls,
        host,
        port,
        target,
    })
}

/// The address a redirection points to (absolute or relative to `base`).
fn redirect_target(base: &Url, location: &str) -> Result<Url, String> {
    let location = location.trim();
    let lower = location.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return parse_url(location);
    }
    if let Some(rest) = location.strip_prefix("//") {
        return parse_url(&format!("{}://{rest}", if base.tls { "https" } else { "http" }));
    }
    let target = if location.starts_with('/') {
        location.to_string()
    } else {
        let dir = base.target.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        format!("{dir}/{location}")
    };
    Ok(Url {
        target,
        ..base.clone()
    })
}

trait Stream: Read + Write {}
impl<T: Read + Write> Stream for T {}

/// The time left before `deadline`, at most `TIMEOUT`.
fn left(deadline: Instant) -> Result<Duration, String> {
    let rest = deadline.saturating_duration_since(Instant::now());
    if rest.is_zero() {
        Err("the server did not answer in time".into())
    } else {
        Ok(rest.min(TIMEOUT))
    }
}

/// The addresses of a host, looked up within the time limit.
fn resolve(url: &Url, deadline: Instant) -> Result<Vec<SocketAddr>, String> {
    let (host, port) = (url.host.clone(), url.port);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(
            (host.as_str(), port)
                .to_socket_addrs()
                .map(|a| a.collect::<Vec<_>>()),
        );
    });
    match rx.recv_timeout(left(deadline)?) {
        Ok(Ok(addrs)) => Ok(addrs),
        Ok(Err(e)) => Err(format!("{}: {e}", url.host)),
        Err(_) => Err(format!("{}: the name could not be looked up in time", url.host)),
    }
}

/// A connection to the server of `url`, and its TCP socket (to bound each read).
fn connect(url: &Url, deadline: Instant) -> Result<(Box<dyn Stream>, TcpStream), String> {
    let mut last = format!("{}: no address found", url.host);
    for addr in resolve(url, deadline)? {
        match TcpStream::connect_timeout(&addr, left(deadline)?) {
            Ok(tcp) => {
                tcp.set_read_timeout(Some(left(deadline)?))
                    .map_err(|e| e.to_string())?;
                tcp.set_write_timeout(Some(left(deadline)?))
                    .map_err(|e| e.to_string())?;
                let socket = tcp.try_clone().map_err(|e| e.to_string())?;
                if !url.tls {
                    return Ok((Box::new(tcp), socket));
                }
                let connector = native_tls::TlsConnector::new().map_err(|e| e.to_string())?;
                return connector
                    .connect(&url.host, tcp)
                    .map(|s| (Box::new(s) as Box<dyn Stream>, socket))
                    .map_err(|e| format!("{}: {e}", url.host));
            }
            Err(e) => last = format!("{addr}: {e}"),
        }
    }
    Err(last)
}

/// An answer of the server.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Answer {
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Answer {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// An answer read as its bytes arrive: the head once, then the body (the
/// chunks of a chunked answer are taken as they complete, so a long answer
/// is read in linear time).
#[derive(Default)]
struct Incoming {
    data: Vec<u8>,
    /// The answer once its head is read, and where its body starts.
    head: Option<(Answer, usize)>,
    /// Chunked body: where the next chunk starts, and the body so far.
    pos: usize,
    body: Vec<u8>,
    /// Where the search for the end of the head goes on.
    scanned: usize,
}

impl Incoming {
    fn head(&mut self, closed: bool) -> Result<bool, String> {
        if self.head.is_some() {
            return Ok(true);
        }
        let from = self.scanned.saturating_sub(3);
        let Some(i) = find(&self.data[from..], b"\r\n\r\n") else {
            self.scanned = self.data.len();
            return if closed {
                Err("the server closed the connection without answering".into())
            } else {
                Ok(false)
            };
        };
        let head_end = from + i;
        let head = String::from_utf8_lossy(&self.data[..head_end]);
        let mut lines = head.split("\r\n");
        let status_line = lines.next().unwrap_or_default();
        let mut parts = status_line.splitn(3, ' ');
        let version = parts.next().unwrap_or_default();
        if !version.starts_with("HTTP/") {
            return Err("the server did not answer with HTTP".into());
        }
        let status: u16 = parts
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or("the answer of the server cannot be read")?;
        let reason = parts.next().unwrap_or_default().to_string();
        let headers: Vec<(String, String)> = lines
            .filter_map(|l| l.split_once(':'))
            .map(|(n, v)| (n.trim().to_string(), v.trim().to_string()))
            .collect();
        let answer = Answer {
            status,
            reason,
            headers,
            body: Vec::new(),
        };
        self.pos = head_end + 4;
        self.head = Some((answer, head_end + 4));
        Ok(true)
    }

    /// Take the chunks that have arrived; `true` once the last one has.
    fn chunks(&mut self) -> Result<bool, String> {
        loop {
            let rest = &self.data[self.pos..];
            let Some(line_end) = find(&rest[..rest.len().min(MAX_CHUNK_LINE)], b"\r\n") else {
                return if rest.len() >= MAX_CHUNK_LINE {
                    Err("a chunk of the answer cannot be read".into())
                } else {
                    Ok(false)
                };
            };
            let size_text =
                std::str::from_utf8(&rest[..line_end]).map_err(|_| "a chunk of the answer cannot be read")?;
            let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
                .map_err(|_| "a chunk of the answer cannot be read".to_string())?;
            if size == 0 {
                return Ok(true);
            }
            // No chunk may make the answer larger than any answer accepted.
            if size > MAX_DOWNLOAD || self.body.len() + size > MAX_DOWNLOAD {
                return Err("the answer of the server is too large".into());
            }
            let start = line_end + 2;
            let Some(after) = start.checked_add(size).and_then(|e| e.checked_add(2)) else {
                return Err("the answer of the server is too large".into());
            };
            if rest.len() < after {
                return Ok(false);
            }
            self.body.extend_from_slice(&rest[start..start + size]);
            self.pos += after;
        }
    }

    /// The whole answer when it has arrived (`closed`: the server closed the
    /// connection, so an answer without a length ends here).
    fn answer(&mut self, closed: bool) -> Result<Option<Answer>, String> {
        if !self.head(closed)? {
            return Ok(None);
        }
        let (answer, body_at) = self.head.as_ref().expect("head");
        let chunked = answer
            .header("Transfer-Encoding")
            .map(|v| v.to_ascii_lowercase().contains("chunked"))
            .unwrap_or(false);
        let length = answer
            .header("Content-Length")
            .and_then(|v| v.parse::<usize>().ok());
        let status = answer.status;
        let body_at = *body_at;
        let body = if chunked {
            if !self.chunks()? {
                return if closed {
                    Err("the answer of the server is incomplete".into())
                } else {
                    Ok(None)
                };
            }
            std::mem::take(&mut self.body)
        } else if let Some(len) = length {
            let rest = &self.data[body_at..];
            if rest.len() < len {
                return if closed {
                    Err("the answer of the server is incomplete".into())
                } else {
                    Ok(None)
                };
            }
            rest[..len].to_vec()
        } else if closed || matches!(status, 204 | 304) {
            self.data[body_at..].to_vec()
        } else {
            return Ok(None);
        };
        let (mut answer, _) = self.head.take().expect("head");
        answer.body = body;
        Ok(Some(answer))
    }
}

/// The answer in `data`, when it is complete (`closed`: the server closed the
/// connection, so an answer without a length ends here).
#[cfg(test)]
fn parse_answer(data: &[u8], closed: bool) -> Result<Option<Answer>, String> {
    let mut incoming = Incoming {
        data: data.to_vec(),
        ..Incoming::default()
    };
    incoming.answer(closed)
}

/// Read the answer to a request sent on `stream` (any connection: a local
/// socket of the print server, TCP), at most `MAX_ANSWER` bytes; the
/// connection's own timeouts bound the wait. Used by the print server
/// client, which Windows does not have.
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) fn read_answer(stream: &mut dyn Read) -> Result<Answer, String> {
    let mut incoming = Incoming::default();
    let mut buf = [0u8; 16 * 1024];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                incoming.data.extend_from_slice(&buf[..n]);
                if incoming.data.len() > MAX_ANSWER {
                    return Err("the answer of the server is too large".into());
                }
                if let Some(answer) = incoming.answer(false)? {
                    return Ok(answer);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                return Err("the server did not answer in time".into());
            }
            Err(_) if !incoming.data.is_empty() => break,
            Err(e) => return Err(e.to_string()),
        }
    }
    incoming
        .answer(true)?
        .ok_or_else(|| "the answer of the server is incomplete".into())
}

/// How a request is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Method {
    Get,
    Post,
}

#[allow(clippy::too_many_arguments)]
fn send_once(
    url: &Url,
    method: Method,
    content_type: &str,
    accept: &str,
    body: &[u8],
    max: usize,
    deadline: Instant,
) -> Result<Answer, String> {
    let (mut stream, socket) = connect(url, deadline)?;
    let host = if url.host.contains(':') {
        format!("[{}]", url.host)
    } else {
        url.host.clone()
    };
    let default_port = if url.tls { 443 } else { 80 };
    let host_header = if url.port == default_port {
        host
    } else {
        format!("{host}:{}", url.port)
    };
    let head = match method {
        Method::Post => format!(
            "POST {} HTTP/1.1\r\nHost: {host_header}\r\nUser-Agent: Folitecta/{}\r\nContent-Type: {content_type}\r\nAccept: {accept}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            url.target,
            folitecta_core::VERSION,
            body.len()
        ),
        Method::Get => format!(
            "GET {} HTTP/1.1\r\nHost: {host_header}\r\nUser-Agent: Folitecta/{}\r\nAccept: {accept}\r\nConnection: close\r\n\r\n",
            url.target,
            folitecta_core::VERSION,
        ),
    };
    let mut request = head.into_bytes();
    if method == Method::Post {
        request.extend_from_slice(body);
    }
    stream.write_all(&request).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;
    let mut incoming = Incoming::default();
    let mut buf = [0u8; 16 * 1024];
    loop {
        socket
            .set_read_timeout(Some(left(deadline)?))
            .map_err(|e| e.to_string())?;
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                incoming.data.extend_from_slice(&buf[..n]);
                if incoming.data.len() > max {
                    return Err("the answer of the server is too large".into());
                }
                if let Some(answer) = incoming.answer(false)? {
                    return Ok(answer);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                return Err("the server did not answer in time".into());
            }
            // A TLS connection closed without its closing message ends the answer too.
            Err(_) if !incoming.data.is_empty() => break,
            Err(e) => return Err(e.to_string()),
        }
    }
    incoming
        .answer(true)?
        .ok_or_else(|| "the answer of the server is incomplete".into())
}

/// Send a request to `url`, following redirections, and return the body of
/// a successful answer.
fn request(
    url: &str,
    method: Method,
    content_type: &str,
    accept: &str,
    body: &[u8],
    max: usize,
) -> Result<Vec<u8>, String> {
    let mut target = parse_url(url)?;
    for _ in 0..4 {
        let deadline = Instant::now() + DEADLINE;
        let answer = send_once(&target, method, content_type, accept, body, max, deadline)?;
        match answer.status {
            200..=299 => return Ok(answer.body),
            301 | 302 | 303 | 307 | 308 => {
                let location = answer
                    .header("Location")
                    .ok_or("the server sent a redirection without an address")?;
                target = redirect_target(&target, location)?;
            }
            status => {
                return Err(format!("the server answered {status} {}", answer.reason)
                    .trim_end()
                    .to_string());
            }
        }
    }
    Err("the server redirected the request too many times".into())
}

/// POST `body` to `url` and return the body of a successful answer.
pub fn post(url: &str, content_type: &str, accept: &str, body: &[u8]) -> Result<Vec<u8>, String> {
    request(url, Method::Post, content_type, accept, body, MAX_ANSWER)
}

/// GET `url` (no body) and return the body of a successful answer (up to
/// 32 MiB, for revocation lists).
pub fn get(url: &str, accept: &str) -> Result<Vec<u8>, String> {
    request(url, Method::Get, "", accept, &[], MAX_DOWNLOAD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn addresses_are_split() {
        let u = parse_url("https://tsa.example.org/tsr?x=1").unwrap();
        assert_eq!(
            (u.tls, u.host.as_str(), u.port, u.target.as_str()),
            (true, "tsa.example.org", 443, "/tsr?x=1")
        );
        let u = parse_url("HTTP://127.0.0.1:8080").unwrap();
        assert_eq!(
            (u.tls, u.host.as_str(), u.port, u.target.as_str()),
            (false, "127.0.0.1", 8080, "/")
        );
        let u = parse_url("http://[::1]:9000/a").unwrap();
        assert_eq!((u.host.as_str(), u.port), ("::1", 9000));
        assert!(parse_url("ftp://example.org").is_err());
        assert!(parse_url("http://user@example.org").is_err());
        let r = redirect_target(&parse_url("http://h/a/b").unwrap(), "c").unwrap();
        assert_eq!(r.target, "/a/c");
        let r = redirect_target(&parse_url("http://h/a/b").unwrap(), "https://k/d").unwrap();
        assert!(r.tls && r.host == "k");
    }

    #[test]
    fn answers_are_read() {
        let a = parse_answer(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabcdef", false)
            .unwrap()
            .unwrap();
        assert_eq!(a.body, b"abc");
        assert!(
            parse_answer(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nabc", false)
                .unwrap()
                .is_none()
        );
        assert!(parse_answer(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nabc", true).is_err());
        let a = parse_answer(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2;x=y\r\nde\r\n0\r\n\r\n",
            false,
        )
        .unwrap()
        .unwrap();
        assert_eq!(a.body, b"abcde");
        let a = parse_answer(b"HTTP/1.0 200 OK\r\n\r\nall of it", true)
            .unwrap()
            .unwrap();
        assert_eq!(a.body, b"all of it");
    }

    /// A server on this computer answering each connection with the next of `answers`.
    fn serve(answers: Vec<Vec<u8>>) -> (String, std::thread::JoinHandle<Vec<Vec<u8>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for answer in answers {
                let (mut s, _) = listener.accept().unwrap();
                let mut data = Vec::new();
                let mut buf = [0u8; 4096];
                // Read the head and the body announced by Content-Length.
                loop {
                    let n = s.read(&mut buf).unwrap();
                    data.extend_from_slice(&buf[..n]);
                    if let Some(end) = find(&data, b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&data[..end]).to_ascii_lowercase();
                        let len = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:"))
                            .and_then(|v| v.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        if data.len() >= end + 4 + len {
                            break;
                        }
                    }
                    if n == 0 {
                        break;
                    }
                }
                requests.push(data);
                s.write_all(&answer).unwrap();
            }
            requests
        });
        (format!("http://{addr}"), handle)
    }

    #[test]
    fn posts_follow_redirections() {
        let (base, server) = serve(vec![
            b"HTTP/1.1 307 Temporary Redirect\r\nLocation: /second\r\nContent-Length: 0\r\n\r\n".to_vec(),
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\npong\r\n0\r\n\r\n".to_vec(),
        ]);
        let body = post(
            &format!("{base}/first"),
            "application/timestamp-query",
            "application/timestamp-reply",
            b"ping",
        )
        .unwrap();
        assert_eq!(body, b"pong");
        let requests = server.join().unwrap();
        let first = String::from_utf8_lossy(&requests[0]);
        assert!(first.starts_with("POST /first HTTP/1.1\r\n"));
        assert!(first.contains("Content-Type: application/timestamp-query\r\n"));
        assert!(first.ends_with("\r\n\r\nping"));
        assert!(String::from_utf8_lossy(&requests[1]).starts_with("POST /second "));
    }

    #[test]
    fn errors_of_the_server_are_reported() {
        let (base, server) = serve(vec![
            b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n".to_vec(),
        ]);
        let err = post(&base, "a/b", "a/c", b"x").unwrap_err();
        assert_eq!(err, "the server answered 500 Internal Server Error");
        server.join().unwrap();
    }

    #[test]
    fn gets_follow_redirections_and_send_no_body() {
        let (base, server) = serve(vec![
            b"HTTP/1.1 302 Found\r\nLocation: /b\r\nContent-Length: 0\r\n\r\n".to_vec(),
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\ncrl\r\n0\r\n\r\n".to_vec(),
        ]);
        let body = get(&format!("{base}/a"), "application/pkix-crl").unwrap();
        assert_eq!(body, b"crl");
        let requests = server.join().unwrap();
        let first = String::from_utf8_lossy(&requests[0]);
        assert!(first.starts_with("GET /a HTTP/1.1\r\n"), "{first}");
        assert!(first.contains("Accept: application/pkix-crl\r\n"));
        assert!(!first.contains("Content-Type") && !first.contains("Content-Length"));
        assert!(first.ends_with("\r\n\r\n"));
        assert!(String::from_utf8_lossy(&requests[1]).starts_with("GET /b "));
        // A redirection to another scheme is refused.
        let (base, server) = serve(vec![
            b"HTTP/1.1 302 Found\r\nLocation: ldap://x/y\r\nContent-Length: 0\r\n\r\n".to_vec(),
        ]);
        assert!(get(&base, "a/b").is_err());
        server.join().unwrap();
    }

    #[test]
    fn long_chunked_answers_are_read_whole() {
        // Many small chunks: each is taken once as it arrives.
        let mut answer = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
        let mut expected = Vec::new();
        for i in 0..20_000u32 {
            let piece = format!("{i:08}");
            answer.extend_from_slice(format!("{:x}\r\n{piece}\r\n", piece.len()).as_bytes());
            expected.extend_from_slice(piece.as_bytes());
        }
        answer.extend_from_slice(b"0\r\n\r\n");
        let started = Instant::now();
        let mut incoming = Incoming::default();
        let mut got = None;
        for part in answer.chunks(4096) {
            incoming.data.extend_from_slice(part);
            if let Some(a) = incoming.answer(false).unwrap() {
                got = Some(a);
                break;
            }
        }
        assert_eq!(got.unwrap().body, expected);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn slow_servers_are_stopped_by_the_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = s.read(&mut buf);
            // One byte every 300 ms, never the end of the answer.
            for b in b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".iter()
            {
                if s.write_all(&[*b]).is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(300));
            }
        });
        let url = parse_url(&format!("http://{addr}/slow")).unwrap();
        let started = Instant::now();
        let err = send_once(
            &url,
            Method::Get,
            "",
            "a/b",
            &[],
            MAX_DOWNLOAD,
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap_err();
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(err, "the server did not answer in time");
        drop(server);
    }

    /// Answers cut, chunked with any sizes (the largest numbers among them) or
    /// with any lengths are errors or incomplete, never a panic.
    #[test]
    fn any_framing_of_an_answer_is_read_without_panicking() {
        let mut x: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let sizes = [
            "ffffffffffffffff",
            "fffffffffffffffe",
            "7fffffffffffffff",
            "10000000000000000",
            "0",
            "3",
            "zz",
            "",
        ];
        for round in 0..20_000 {
            let mut data = b"HTTP/1.1 200 OK\r\n".to_vec();
            if round % 2 == 0 {
                data.extend_from_slice(b"Transfer-Encoding: chunked\r\n\r\n");
                for _ in 0..(next() % 4) {
                    let size = if next() % 3 == 0 {
                        sizes[(next() % sizes.len() as u64) as usize].to_string()
                    } else {
                        format!("{:x}", next() % 40)
                    };
                    data.extend_from_slice(size.as_bytes());
                    data.extend_from_slice(b"\r\n");
                    for _ in 0..(next() % 50) {
                        data.push((next() % 256) as u8);
                    }
                    if next() % 2 == 0 {
                        data.extend_from_slice(b"\r\n");
                    }
                }
            } else {
                data.extend_from_slice(format!("Content-Length: {}\r\n\r\n", next() % 100).as_bytes());
                for _ in 0..(next() % 80) {
                    data.push((next() % 256) as u8);
                }
            }
            let cut = (next() as usize) % (data.len() + 1);
            for closed in [false, true] {
                let _ = parse_answer(&data[..cut], closed);
            }
            let _ = read_answer(&mut std::io::Cursor::new(data[..cut].to_vec()));
        }
        let whole = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n".to_vec();
        assert_eq!(
            read_answer(&mut std::io::Cursor::new(whole)).unwrap().body,
            b"abc"
        );
    }

    #[test]
    fn huge_chunk_sizes_are_refused() {
        // A chunk size near the largest number: refused, never an overflow.
        for size in ["fffffffffffffffe", "ffffffffffffffff", "2000001"] {
            let answer = format!(
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{size}\r\nabcdefghijklmnopqrstuvwxyz0123456789\r\n0\r\n\r\n"
            );
            let err = parse_answer(answer.as_bytes(), true).unwrap_err();
            assert_eq!(err, "the answer of the server is too large", "{size}");
        }
        // Through a server on this computer, as the probe sends it.
        let (base, server) = serve(vec![
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nfffffffffffffffe\r\nabcdefghijklmnopqrstuvwxyz0123456789\r\n0\r\n\r\n".to_vec(),
        ]);
        assert!(get(&format!("{base}/list.crl"), "application/pkix-crl").is_err());
        server.join().unwrap();
        // A size line that never ends.
        let mut endless = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
        endless.extend(std::iter::repeat_n(b'1', 4096));
        assert!(parse_answer(&endless, false).is_err());
    }
}
