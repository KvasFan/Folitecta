//! A small HTTP/1.1 client for the requests Folitecta sends on its own (time
//! stamps): one POST, plain or over the system's TLS (native-tls), following
//! a few redirections. Nothing is sent unless the user set the address.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(20);
/// Largest answer accepted.
const MAX_ANSWER: usize = 8 * 1024 * 1024;

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

fn connect(url: &Url) -> Result<Box<dyn Stream>, String> {
    let addrs = (url.host.as_str(), url.port)
        .to_socket_addrs()
        .map_err(|e| format!("{}: {e}", url.host))?;
    let mut last = format!("{}: no address found", url.host);
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, TIMEOUT) {
            Ok(tcp) => {
                tcp.set_read_timeout(Some(TIMEOUT)).map_err(|e| e.to_string())?;
                tcp.set_write_timeout(Some(TIMEOUT)).map_err(|e| e.to_string())?;
                if !url.tls {
                    return Ok(Box::new(tcp));
                }
                let connector = native_tls::TlsConnector::new().map_err(|e| e.to_string())?;
                return connector
                    .connect(&url.host, tcp)
                    .map(|s| Box::new(s) as Box<dyn Stream>)
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

/// The body of a chunked answer, once every chunk has arrived.
fn dechunk(mut data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let line_end = find(data, b"\r\n")?;
        let size_text = std::str::from_utf8(&data[..line_end]).ok()?;
        let size = usize::from_str_radix(size_text.split(';').next()?.trim(), 16).ok()?;
        data = &data[line_end + 2..];
        if size == 0 {
            return Some(out);
        }
        // A size past the data (or past the largest number) is an answer not complete yet.
        let end = size.checked_add(2).filter(|end| *end <= data.len())?;
        out.extend_from_slice(&data[..size]);
        data = &data[end..];
    }
}

/// The answer in `data`, when it is complete (`closed`: the server closed the
/// connection, so an answer without a length ends here).
pub(crate) fn parse_answer(data: &[u8], closed: bool) -> Result<Option<Answer>, String> {
    let Some(head_end) = find(data, b"\r\n\r\n") else {
        return if closed {
            Err("the server closed the connection without answering".into())
        } else {
            Ok(None)
        };
    };
    let head = String::from_utf8_lossy(&data[..head_end]);
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
    let mut answer = Answer {
        status,
        reason,
        headers,
        body: Vec::new(),
    };
    let rest = &data[head_end + 4..];
    let chunked = answer
        .header("Transfer-Encoding")
        .map(|v| v.to_ascii_lowercase().contains("chunked"))
        .unwrap_or(false);
    if chunked {
        return match dechunk(rest) {
            Some(body) => {
                answer.body = body;
                Ok(Some(answer))
            }
            None if closed => Err("the answer of the server is incomplete".into()),
            None => Ok(None),
        };
    }
    if let Some(len) = answer
        .header("Content-Length")
        .and_then(|v| v.parse::<usize>().ok())
    {
        if rest.len() >= len {
            answer.body = rest[..len].to_vec();
            return Ok(Some(answer));
        }
        return if closed {
            Err("the answer of the server is incomplete".into())
        } else {
            Ok(None)
        };
    }
    if closed || matches!(status, 204 | 304) {
        answer.body = rest.to_vec();
        return Ok(Some(answer));
    }
    Ok(None)
}

fn send_once(url: &Url, content_type: &str, accept: &str, body: &[u8]) -> Result<Answer, String> {
    let mut stream = connect(url)?;
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
    let head = format!(
        "POST {} HTTP/1.1\r\nHost: {host_header}\r\nUser-Agent: Folitecta/{}\r\nContent-Type: {content_type}\r\nAccept: {accept}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        url.target,
        folitecta_core::VERSION,
        body.len()
    );
    let mut request = head.into_bytes();
    request.extend_from_slice(body);
    stream.write_all(&request).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;
    read_answer(&mut stream)
}

/// Read the answer to a request sent on `stream` (any connection: TCP, TLS,
/// a local socket).
pub(crate) fn read_answer(stream: &mut dyn Read) -> Result<Answer, String> {
    let mut data = Vec::new();
    let mut buf = [0u8; 16 * 1024];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                data.extend_from_slice(&buf[..n]);
                if data.len() > MAX_ANSWER {
                    return Err("the answer of the server is too large".into());
                }
                if let Some(answer) = parse_answer(&data, false)? {
                    return Ok(answer);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            // A TLS connection closed without its closing message ends the answer too.
            Err(e)
                if !data.is_empty()
                    && e.kind() != std::io::ErrorKind::TimedOut
                    && e.kind() != std::io::ErrorKind::WouldBlock =>
            {
                break;
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    parse_answer(&data, true)?.ok_or_else(|| "the answer of the server is incomplete".into())
}

/// POST `body` to `url` and return the body of a successful answer.
pub fn post(url: &str, content_type: &str, accept: &str, body: &[u8]) -> Result<Vec<u8>, String> {
    let mut target = parse_url(url)?;
    for _ in 0..4 {
        let answer = send_once(&target, content_type, accept, body)?;
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
                let _ = read_answer(&mut std::io::Cursor::new(data[..cut].to_vec()));
            }
        }
        assert_eq!(dechunk(b"ffffffffffffffff\r\nabc"), None);
        assert_eq!(dechunk(b"3\r\nabc\r\n0\r\n\r\n"), Some(b"abc".to_vec()));
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
}
