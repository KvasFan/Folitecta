//! speech-dispatcher, the speech service of Linux desktops (the one screen
//! readers use), spoken to over its Unix socket in its own protocol (SSIP);
//! no library is linked. When its socket is missing the service is started
//! the way its own client library does (`speech-dispatcher --spawn`), once.
//!
//! The protocol, as the server implements it: every line ends with CR LF; a
//! reply is lines `ccc-data` and a last line `ccc text`; events (codes 7xx:
//! 701 begin, 702 end, 703 canceled, 704 paused, 705 resumed, 700 index
//! mark; data: the message id, then the client id) come at any time, also
//! between a command and its reply. So one thread reads the socket and
//! passes every block to the thread that talks to the service, which takes
//! the events apart from the replies. Text is sent after `SPEAK` and a `230`
//! reply, ends with a line holding a single dot (a dot starting the text is
//! doubled), and is answered `225-<message id>`.
//!
//! Pausing is not done with `PAUSE` (it waits for the end of a sentence,
//! and a later `CANCEL` does not undo it): a paused paragraph is cancelled
//! and spoken again from its start (see [`super::resumes_in_place`]).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use super::{Spoken, Voice, tell};

/// A reply or an event: its code, its data lines, its last text.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Block {
    pub code: u16,
    pub data: Vec<String>,
    pub text: String,
}

impl Block {
    fn ok(&self) -> bool {
        (200..300).contains(&self.code)
    }

    fn is_event(&self) -> bool {
        (700..800).contains(&self.code)
    }
}

/// What the thread of the service is asked or told.
enum Msg {
    Block(Block),
    /// The socket was closed.
    Closed,
    Voices(Sender<Option<Vec<Voice>>>),
    Speak {
        id: u64,
        text: String,
        voice: String,
        rate: f32,
    },
    Cancel,
    /// Say goodbye and close the connection.
    Quit,
}

/// The next block of the socket; `None` at its end.
pub(crate) fn read_block(reader: &mut impl BufRead) -> Option<Block> {
    let mut data = Vec::new();
    loop {
        let mut line = Vec::new();
        if reader.read_until(b'\n', &mut line).ok()? == 0 {
            return None;
        }
        let line = String::from_utf8_lossy(&line);
        let line = line.trim_end_matches(['\r', '\n']);
        if line.len() < 4 || !line.is_char_boundary(3) || !line.is_char_boundary(4) {
            continue;
        }
        let Ok(code) = line[..3].parse::<u16>() else {
            continue;
        };
        let rest = line[4..].to_string();
        if line.as_bytes()[3] == b'-' {
            data.push(rest);
        } else {
            return Some(Block {
                code,
                data,
                text: rest,
            });
        }
    }
}

/// The text of a message as sent after `SPEAK`: its lines joined with
/// spaces (a paragraph needs no line breaks), a leading dot doubled, ended
/// by a line holding a single dot.
pub(crate) fn message(text: &str) -> String {
    let one = text
        .split(['\r', '\n'])
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let one = if one.starts_with('.') {
        format!(".{one}")
    } else {
        one
    };
    format!("{one}\r\n.\r\n")
}

/// The speed of speech-dispatcher (-100 to 100, 0 normal) for a rate of
/// speech (0.5 to 2, 1 normal): half and double speed at the ends.
pub(crate) fn ssip_rate(rate: f32) -> i32 {
    (rate.clamp(0.5, 2.0).log2() * 100.0).round() as i32
}

/// The voices of a `LIST SYNTHESIS_VOICES` reply (`name TAB language TAB
/// variant`). The voice of the dummy module alone means there is no voice
/// (it would play a recording saying speech is not set up).
pub(crate) fn parse_voices(b: &Block) -> Option<Vec<Voice>> {
    let voices: Vec<Voice> = b
        .data
        .iter()
        .filter_map(|l| {
            let mut parts = l.split('\t');
            let name = parts.next()?.to_string();
            let lang = parts.next().filter(|l| *l != "none").unwrap_or("").to_string();
            (!name.is_empty()).then(|| Voice {
                id: name.clone(),
                name,
                lang,
            })
        })
        .collect();
    let dummy = voices.len() == 1 && voices[0].name == "dummy";
    (!voices.is_empty() && !dummy).then_some(voices)
}

/// The socket of the service: `SPEECHD_ADDRESS` when it names a Unix
/// socket, else the one in the runtime folder (or the cache folder).
pub(crate) fn socket_path() -> Option<PathBuf> {
    if let Ok(addr) = std::env::var("SPEECHD_ADDRESS") {
        let mut parts = addr.split(':');
        return match (parts.next(), parts.next()) {
            (Some("unix_socket"), Some(p)) if !p.is_empty() => Some(PathBuf::from(p)),
            (Some("unix_socket"), _) => default_socket(),
            // Network sockets are not used.
            _ => None,
        };
    }
    default_socket()
}

fn default_socket() -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(dir.join("speech-dispatcher").join("speechd.sock"))
}

/// Start the service the way its client library does and wait for it to be
/// ready: the starting process ends once the socket listens.
fn spawn_service(socket: &Path) -> bool {
    let cmd = std::env::var("SPEECHD_CMD").unwrap_or_else(|_| "speech-dispatcher".into());
    std::process::Command::new(cmd)
        .args([
            "--spawn",
            "--communication-method",
            "unix_socket",
            "--socket-path",
        ])
        .arg(socket)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// A connection to the service, talked to by one thread.
struct Session {
    stream: UnixStream,
    rx: Receiver<Msg>,
    /// Commands that came while a reply was awaited.
    later: Vec<Msg>,
    client_id: Option<String>,
    /// The utterance of each message id spoken, and the ends of messages
    /// that came before their reply told their id.
    spoken: HashMap<String, u64>,
    early: Vec<Block>,
    /// The voice and speed last set, and the voices listed.
    voice: Option<String>,
    rate: Option<i32>,
    voices: Vec<Voice>,
}

impl Session {
    /// Send a command and wait for its reply, handling the events that come first.
    fn request(&mut self, line: &str) -> Option<Block> {
        self.stream.write_all(format!("{line}\r\n").as_bytes()).ok()?;
        self.reply()
    }

    fn reply(&mut self) -> Option<Block> {
        loop {
            match self.rx.recv_timeout(Duration::from_secs(10)) {
                Ok(Msg::Block(b)) if b.is_event() => self.event(b),
                Ok(Msg::Block(b)) => return Some(b),
                Ok(Msg::Closed) | Err(_) => return None,
                Ok(other) => self.later.push(other),
            }
        }
    }

    /// An event of one of this connection's messages: its end or its
    /// cancellation is told.
    fn event(&mut self, b: Block) {
        let (Some(msg), client) = (b.data.first().cloned(), b.data.get(1)) else {
            return;
        };
        if let (Some(mine), Some(theirs)) = (&self.client_id, client) {
            if mine != theirs {
                return;
            }
        }
        let Some(&id) = self.spoken.get(&msg) else {
            // Its reply has not come yet.
            if matches!(b.code, 702 | 703) {
                self.early.push(b);
            }
            return;
        };
        match b.code {
            702 => {
                self.spoken.remove(&msg);
                tell(Spoken::Ended(id));
            }
            703 => {
                self.spoken.remove(&msg);
                tell(Spoken::Failed(id, "canceled".into()));
            }
            _ => {}
        }
    }

    fn start(&mut self) -> bool {
        let user: String = std::env::var("USER")
            .unwrap_or_default()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        let user = if user.is_empty() { "user".to_string() } else { user };
        let named = self
            .request(&format!("SET self CLIENT_NAME {user}:folitecta:read"))
            .is_some_and(|b| b.ok());
        self.client_id = self
            .request("HISTORY GET CLIENT_ID")
            .filter(|b| b.ok())
            .and_then(|b| b.data.first().cloned());
        // Before any SPEAK: the events of a message are chosen when it is queued.
        let told = self
            .request("SET self NOTIFICATION end on")
            .is_some_and(|b| b.ok())
            && self
                .request("SET self NOTIFICATION cancel on")
                .is_some_and(|b| b.ok());
        // One paragraph at a time, each sent when the one before ends.
        let _ = self.request("SET self PRIORITY text");
        named && told
    }

    fn list_voices(&mut self) -> Option<Vec<Voice>> {
        let b = self.request("LIST SYNTHESIS_VOICES")?;
        let voices = if b.ok() { parse_voices(&b) } else { None };
        self.voices = voices.clone().unwrap_or_default();
        voices
    }

    fn speak(&mut self, id: u64, text: &str, voice: &str, rate: f32) {
        let r = ssip_rate(rate);
        if self.rate != Some(r)
            && self
                .request(&format!("SET self RATE {r}"))
                .is_some_and(|b| b.ok())
        {
            self.rate = Some(r);
        }
        if !voice.is_empty() && self.voice.as_deref() != Some(voice) {
            // The language first: changing it may change the voice.
            let lang = self
                .voices
                .iter()
                .find(|v| v.id == voice)
                .map(|v| v.lang.clone())
                .filter(|l| !l.is_empty());
            if let Some(lang) = lang {
                let _ = self.request(&format!("SET self LANGUAGE {lang}"));
            }
            if self
                .request(&format!("SET self SYNTHESIS_VOICE {voice}"))
                .is_some_and(|b| b.ok())
            {
                self.voice = Some(voice.to_string());
            }
        }
        let failed = || tell(Spoken::Failed(id, "speech-dispatcher".into()));
        if !self.request("SPEAK").is_some_and(|b| b.code == 230) {
            failed();
            return;
        }
        if self.stream.write_all(message(text).as_bytes()).is_err() {
            failed();
            return;
        }
        match self.reply() {
            Some(b) if b.code == 225 => {
                let Some(msg) = b.data.first().cloned() else {
                    return;
                };
                self.spoken.insert(msg, id);
                // Ends that came before the reply told the message id.
                for e in std::mem::take(&mut self.early) {
                    self.event(e);
                }
            }
            _ => failed(),
        }
    }

    fn run(mut self) {
        loop {
            let msg = if self.later.is_empty() {
                match self.rx.recv() {
                    Ok(m) => m,
                    Err(_) => break,
                }
            } else {
                self.later.remove(0)
            };
            match msg {
                Msg::Block(b) if b.is_event() => self.event(b),
                Msg::Block(_) => {}
                Msg::Closed => break,
                Msg::Voices(reply) => {
                    let _ = reply.send(self.list_voices());
                }
                Msg::Speak {
                    id,
                    text,
                    voice,
                    rate,
                } => self.speak(id, &text, &voice, rate),
                Msg::Cancel => {
                    let _ = self.request("CANCEL self");
                }
                Msg::Quit => {
                    let _ = self.request("CANCEL self");
                    let _ = self.stream.write_all(b"QUIT\r\n");
                    break;
                }
            }
        }
        let _ = self.stream.shutdown(Shutdown::Both);
        // The service is gone: what was being spoken will not end.
        for (_, id) in self.spoken.drain() {
            tell(Spoken::Failed(id, "speech-dispatcher".into()));
        }
        *SESSION.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// The thread of the open connection.
static SESSION: Mutex<Option<Sender<Msg>>> = Mutex::new(None);

/// Connect to the socket at `path` and start the threads of the connection.
fn open(path: &Path) -> Option<Sender<Msg>> {
    let stream = UnixStream::connect(path).ok()?;
    let reading = stream.try_clone().ok()?;
    let (tx, rx) = channel();
    let blocks = tx.clone();
    std::thread::Builder::new()
        .name("speech-dispatcher reader".into())
        .spawn(move || {
            let mut reader = BufReader::new(reading);
            while let Some(b) = read_block(&mut reader) {
                if blocks.send(Msg::Block(b)).is_err() {
                    return;
                }
            }
            let _ = blocks.send(Msg::Closed);
        })
        .ok()?;
    let mut session = Session {
        stream,
        rx,
        later: Vec::new(),
        client_id: None,
        spoken: HashMap::new(),
        early: Vec::new(),
        voice: None,
        rate: None,
        voices: Vec::new(),
    };
    if !session.start() {
        let _ = session.stream.shutdown(Shutdown::Both);
        return None;
    }
    std::thread::Builder::new()
        .name("speech-dispatcher".into())
        .spawn(move || session.run())
        .ok()?;
    Some(tx)
}

/// The connection, opened (and the service started) when needed.
fn session() -> Option<Sender<Msg>> {
    let mut s = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(tx) = s.clone() {
        return Some(tx);
    }
    let path = socket_path()?;
    let tx = open(&path).or_else(|| spawn_service(&path).then(|| open(&path)).flatten())?;
    *s = Some(tx.clone());
    Some(tx)
}

/// The voices of the service (call away from the interface thread); None
/// when there is no service or it has no voice.
pub fn voices() -> Option<Vec<Voice>> {
    let tx = session()?;
    let (reply, answer) = channel();
    tx.send(Msg::Voices(reply)).ok()?;
    answer.recv_timeout(Duration::from_secs(10)).ok().flatten()
}

/// What the interface asks of the service, done in order by one thread:
/// opening the connection (maybe starting the service) never holds up the
/// interface.
enum Call {
    Speak {
        id: u64,
        text: String,
        voice: String,
        rate: f32,
    },
    Cancel,
    Close,
}

fn call(c: Call) {
    static WORKER: Mutex<Option<Sender<Call>>> = Mutex::new(None);
    let mut w = WORKER.lock().unwrap_or_else(|e| e.into_inner());
    let tx = w.get_or_insert_with(|| {
        let (tx, rx) = channel::<Call>();
        let _ = std::thread::Builder::new()
            .name("speech-calls".into())
            .spawn(move || {
                while let Ok(c) = rx.recv() {
                    match c {
                        Call::Speak {
                            id,
                            text,
                            voice,
                            rate,
                        } => speak_now(id, text, voice, rate),
                        Call::Cancel => {
                            if let Some(tx) = SESSION.lock().unwrap_or_else(|e| e.into_inner()).clone() {
                                let _ = tx.send(Msg::Cancel);
                            }
                        }
                        Call::Close => {
                            if let Some(tx) = SESSION.lock().unwrap_or_else(|e| e.into_inner()).take() {
                                let _ = tx.send(Msg::Quit);
                            }
                        }
                    }
                }
            });
        tx
    });
    if let Err(e) = tx.send(c) {
        // The thread is gone: what it was asked to speak fails.
        if let Call::Speak { id, .. } = e.0 {
            tell(Spoken::Failed(id, "speech-dispatcher".into()));
        }
        *w = None;
    }
}

pub fn speak(id: u64, text: &str, voice: &str, rate: f32) {
    call(Call::Speak {
        id,
        text: text.to_string(),
        voice: voice.to_string(),
        rate,
    });
}

fn speak_now(id: u64, text: String, voice: String, rate: f32) {
    let sent = session().is_some_and(|tx| {
        tx.send(Msg::Speak {
            id,
            text,
            voice,
            rate,
        })
        .is_ok()
    });
    if !sent {
        tell(Spoken::Failed(id, "speech-dispatcher".into()));
    }
}

/// Close the connection (the service stops by itself a few seconds after
/// its last client goes).
pub fn close() {
    call(Call::Close);
}

/// Stop what is spoken (and what is queued) for this connection.
pub fn cancel() {
    call(Call::Cancel);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::os::unix::net::UnixListener;

    #[test]
    fn replies_and_events_are_read_as_blocks() {
        let mut r = Cursor::new(
            b"225-42\r\n225 OK MESSAGE QUEUED\r\n702-42\r\n702-7\r\n702 END\r\n208 OK CLIENT NAME SET\r\n"
                .to_vec(),
        );
        let b = read_block(&mut r).unwrap();
        assert_eq!(
            (b.code, b.data.clone(), b.text.as_str()),
            (225, vec!["42".to_string()], "OK MESSAGE QUEUED")
        );
        let e = read_block(&mut r).unwrap();
        assert!(e.is_event() && e.data == ["42", "7"] && e.code == 702);
        assert_eq!(read_block(&mut r).unwrap().code, 208);
        assert!(read_block(&mut r).is_none());
    }

    #[test]
    fn messages_are_one_line_with_a_leading_dot_doubled() {
        assert_eq!(message("Hello.\nWorld"), "Hello. World\r\n.\r\n");
        assert_eq!(message(".hidden"), "..hidden\r\n.\r\n");
        assert_eq!(message("a\r\n.\r\nb"), "a . b\r\n.\r\n");
    }

    #[test]
    fn rates_and_voices() {
        assert_eq!((ssip_rate(0.5), ssip_rate(1.0), ssip_rate(2.0)), (-100, 0, 100));
        let b = Block {
            code: 249,
            data: vec!["French (France)\tfr-FR\tnone".into(), "Italiano\tit\tnone".into()],
            text: "OK VOICE LIST SENT".into(),
        };
        let v = parse_voices(&b).unwrap();
        assert_eq!(
            (v[0].name.as_str(), v[0].lang.as_str()),
            ("French (France)", "fr-FR")
        );
        let dummy = Block {
            code: 249,
            data: vec!["dummy\ten\tnone".into()],
            text: String::new(),
        };
        assert!(parse_voices(&dummy).is_none());
    }

    /// A service answering like speech-dispatcher: another client's event
    /// before a reply, the end of a message right after its reply.
    fn fake_service(path: &Path) -> std::thread::JoinHandle<Vec<String>> {
        let listener = UnixListener::bind(path).unwrap();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut out = stream.try_clone().unwrap();
            let mut reader = BufReader::new(stream);
            let mut heard = Vec::new();
            let mut line = String::new();
            let mut data = false;
            let mut msg = 40;
            loop {
                line.clear();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                assert!(line.ends_with("\r\n"), "{line:?}");
                let l = line.trim_end().to_string();
                if data {
                    if l == "." {
                        data = false;
                        msg += 1;
                        let ok = format!(
                            "225-{msg}\r\n225 OK MESSAGE QUEUED\r\n701-{msg}\r\n701-7\r\n701 BEGIN\r\n702-{msg}\r\n702-7\r\n702 END\r\n"
                        );
                        if out.write_all(ok.as_bytes()).is_err() {
                            break;
                        }
                    } else {
                        heard.push(l);
                    }
                    continue;
                }
                heard.push(l.clone());
                let reply = match l.as_str() {
                    s if s.starts_with("SET self CLIENT_NAME ") => "208 OK CLIENT NAME SET\r\n",
                    "HISTORY GET CLIENT_ID" => "245-7\r\n245 OK CLIENT ID SENT\r\n",
                    s if s.starts_with("SET self NOTIFICATION ") => "220 OK NOTIFICATION SET\r\n",
                    "SET self PRIORITY text" => "202 OK PRIORITY SET\r\n",
                    "LIST SYNTHESIS_VOICES" => {
                        "249-Italiano\tit\tnone\r\n249-English\ten\tnone\r\n249 OK VOICE LIST SENT\r\n"
                    }
                    s if s.starts_with("SET self RATE ") => "203 OK RATE SET\r\n",
                    s if s.starts_with("SET self LANGUAGE ") => "201 OK LANGUAGE SET\r\n",
                    s if s.starts_with("SET self SYNTHESIS_VOICE ") => "209 OK VOICE SET\r\n",
                    "SPEAK" => {
                        data = true;
                        // Another client's event before the reply: not this one's.
                        "702-99\r\n702-8\r\n702 END\r\n230 OK RECEIVING DATA\r\n"
                    }
                    "CANCEL self" => "213 OK CANCELED\r\n",
                    "QUIT" => "231 HAPPY HACKING\r\n",
                    _ => "500 ERR INVALID COMMAND\r\n",
                };
                // The client may be gone after QUIT.
                if out.write_all(reply.as_bytes()).is_err() {
                    break;
                }
            }
            heard
        })
    }

    #[test]
    fn a_session_lists_voices_speaks_and_hears_the_end() {
        let dir = std::env::temp_dir().join(format!("folitecta-ssip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("speechd.sock");
        let service = fake_service(&path);
        let mut events = super::super::events().expect("the receiver of the ends");
        let tx = open(&path).expect("connected");
        let (reply, answer) = channel();
        tx.send(Msg::Voices(reply)).unwrap();
        let voices = answer.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        assert_eq!(
            voices.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(),
            ["Italiano", "English"]
        );
        tx.send(Msg::Speak {
            id: 5,
            text: ".Buongiorno\na tutti".into(),
            voice: "Italiano".into(),
            rate: 2.0,
        })
        .unwrap();
        // The end of utterance 5 (the other client's end is not told).
        let mut got = None;
        for _ in 0..250 {
            if let Ok(e) = events.try_recv() {
                got = Some(e);
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(got, Some(Spoken::Ended(5)));
        assert!(events.try_recv().is_err(), "only one end");
        tx.send(Msg::Quit).unwrap();
        let heard = service.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        for l in [
            "SET self CLIENT_NAME",
            "SET self NOTIFICATION end on",
            "SET self RATE 100",
            "SET self LANGUAGE it",
            "SET self SYNTHESIS_VOICE Italiano",
            "SPEAK",
            "..Buongiorno a tutti",
            "QUIT",
        ] {
            assert!(heard.iter().any(|h| h.starts_with(l)), "{l} in {heard:?}");
        }
        // The language before the voice, the voice before the text.
        let at = |l: &str| heard.iter().position(|h| h.starts_with(l)).unwrap();
        assert!(
            at("SET self LANGUAGE") < at("SET self SYNTHESIS_VOICE")
                && at("SET self SYNTHESIS_VOICE") < at("SPEAK")
        );
    }
}
