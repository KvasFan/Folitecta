//! Speaking text with a voice of the system (FOL-L042). The speech of the
//! web view is used where it has voices (Windows, macOS); on Linux, where
//! the web view has none, speech-dispatcher (see [`ssip`]); the self-test
//! uses a stand-in that speaks nothing. One utterance is spoken at a time;
//! its end (or failure) comes back through [`events`].

use std::sync::{Mutex, OnceLock};

use dioxus::prelude::*;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

#[cfg(all(unix, not(target_os = "macos")))]
pub mod ssip;

/// A voice an engine offers.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct Voice {
    /// What the engine knows it by.
    pub id: String,
    pub name: String,
    /// Language tag, such as `it-IT` (may be empty).
    #[serde(default)]
    pub lang: String,
}

/// Who speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    /// The speech of the web view.
    Web,
    /// speech-dispatcher, over its socket.
    SpeechDispatcher,
    /// Nothing is spoken (the self-test).
    StandIn,
}

/// The end of an utterance.
#[derive(Debug, Clone, PartialEq)]
pub enum Spoken {
    Ended(u64),
    Failed(u64, String),
}

/// The sending end of the ends of utterances, and its receiving end until
/// it is taken.
type Channel = (UnboundedSender<Spoken>, Mutex<Option<UnboundedReceiver<Spoken>>>);

/// Where the engines tell about their utterances; read by [`events`].
fn channel() -> &'static Channel {
    static CHANNEL: OnceLock<Channel> = OnceLock::new();
    CHANNEL.get_or_init(|| {
        let (tx, rx) = unbounded_channel();
        (tx, Mutex::new(Some(rx)))
    })
}

pub(crate) fn tell(s: Spoken) {
    let _ = channel().0.send(s);
}

/// The receiver of the ends of utterances (taken once, by the reader).
pub fn events() -> Option<UnboundedReceiver<Spoken>> {
    channel().1.lock().unwrap_or_else(|e| e.into_inner()).take()
}

// ---- The stand-in of the self-test -----------------------------------------

static STAND_IN: Mutex<Option<Vec<String>>> = Mutex::new(None);

/// The stand-in is paused: what it says does not end until it goes on.
static STAND_IN_PAUSED: Mutex<bool> = Mutex::new(false);

fn stand_in_paused() -> bool {
    *STAND_IN_PAUSED.lock().unwrap_or_else(|e| e.into_inner())
}

fn set_stand_in_paused(on: bool) {
    *STAND_IN_PAUSED.lock().unwrap_or_else(|e| e.into_inner()) = on;
}

/// Speak nothing from now on, remembering what would be said (`true`), or
/// speak again (`false`).
pub fn set_stand_in(on: bool) {
    *STAND_IN.lock().unwrap_or_else(|e| e.into_inner()) = on.then(Vec::new);
    set_stand_in_paused(false);
}

/// What the stand-in was given to say.
pub fn stood_in() -> Vec<String> {
    STAND_IN
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

fn stand_in_on() -> bool {
    STAND_IN.lock().unwrap_or_else(|e| e.into_inner()).is_some()
}

// ---- The speech of the web view --------------------------------------------

/// Speech of the web view: `window.folitectaSpeech`, telling the end of each
/// utterance through `window.folitectaSpeechSend`. An utterance is kept in
/// a variable until it ends, or some engines drop it with its events.
const WEB_SPEECH_JS: &str = r#"
window.folitectaSpeech = window.folitectaSpeech || (() => {
  const s = window.speechSynthesis;
  let current = null;
  const voices = () => new Promise(done => {
    if (!s) return done([]);
    const now = s.getVoices();
    if (now.length) return done(now);
    let over = false;
    const end = () => { if (!over) { over = true; done(s.getVoices()); } };
    s.addEventListener('voiceschanged', end, { once: true });
    setTimeout(end, 2000);
  });
  const send = m => window.folitectaSpeechSend && window.folitectaSpeechSend(m);
  return {
    voices: async () => (await voices()).map(v => ({ id: v.voiceURI || v.name, name: v.name, lang: v.lang || '' })),
    speak: async (id, text, voice, rate) => {
      if (!s) { send(['error', id, 'no speech']); return; }
      const u = new SpeechSynthesisUtterance(text);
      const v = (await voices()).find(v => (v.voiceURI || v.name) === voice);
      if (v) { u.voice = v; u.lang = v.lang; }
      u.rate = rate;
      u.onend = () => { if (current === u) current = null; send(['end', id]); };
      u.onerror = e => { if (current === u) current = null; send(['error', id, String(e.error || 'error')]); };
      current = u;
      s.speak(u);
      // A paused engine stays paused for the next utterance.
      if (s.paused) s.resume();
    },
    pause: () => { if (s) s.pause(); },
    resume: () => { if (s) s.resume(); },
    cancel: () => { current = null; if (s) s.cancel(); },
  };
})();
"#;

/// The web view's speech is ready: its script runs and its events come back.
static WEB_READY: Mutex<bool> = Mutex::new(false);

async fn web_ready() {
    if *WEB_READY.lock().unwrap_or_else(|e| e.into_inner()) {
        return;
    }
    *WEB_READY.lock().unwrap_or_else(|e| e.into_inner()) = true;
    let _ = document::eval(WEB_SPEECH_JS).await;
    // The events of the utterances, for as long as the window lives.
    dioxus::core::spawn_forever(async move {
        let mut ev =
            document::eval("window.folitectaSpeechSend = m => dioxus.send(m); await new Promise(() => {});");
        while let Ok(m) = ev.recv::<serde_json::Value>().await {
            let Some(m) = m.as_array() else { continue };
            let id = m.get(1).and_then(|v| v.as_u64()).unwrap_or(0);
            match m.first().and_then(|v| v.as_str()) {
                Some("end") => tell(Spoken::Ended(id)),
                Some("error") => {
                    let why = m.get(2).and_then(|v| v.as_str()).unwrap_or("error").to_string();
                    tell(Spoken::Failed(id, why))
                }
                _ => {}
            }
        }
    });
}

async fn web_voices() -> Vec<Voice> {
    web_ready().await;
    match document::eval("return await window.folitectaSpeech.voices();").await {
        Ok(v) => serde_json::from_value(v).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn web_call(js: String) {
    spawn(async move {
        let _ = document::eval(&js).await;
    });
}

// ---- The engine in use -----------------------------------------------------

/// The engine chosen the first time something is read.
static ENGINE: Mutex<Option<EngineKind>> = Mutex::new(None);

/// The engine to speak with and its voices: the stand-in in the self-test,
/// the web view when it has voices, else speech-dispatcher where there is
/// one; None when no engine can speak.
pub async fn choose() -> Option<(EngineKind, Vec<Voice>)> {
    if stand_in_on() {
        let voices = vec![Voice {
            id: "stand-in".into(),
            name: "Stand-in".into(),
            lang: String::new(),
        }];
        *ENGINE.lock().unwrap_or_else(|e| e.into_inner()) = Some(EngineKind::StandIn);
        return Some((EngineKind::StandIn, voices));
    }
    let voices = web_voices().await;
    if !voices.is_empty() {
        *ENGINE.lock().unwrap_or_else(|e| e.into_inner()) = Some(EngineKind::Web);
        return Some((EngineKind::Web, voices));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let voices = tokio::task::spawn_blocking(ssip::voices).await.ok().flatten();
        if let Some(voices) = voices {
            *ENGINE.lock().unwrap_or_else(|e| e.into_inner()) = Some(EngineKind::SpeechDispatcher);
            return Some((EngineKind::SpeechDispatcher, voices));
        }
    }
    *ENGINE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    None
}

fn engine() -> Option<EngineKind> {
    *ENGINE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Speak `text` as utterance `id` with voice `voice` (an id of [`choose`]'s
/// voices; empty: the engine's own) at `rate` (1 normal, 0.5 to 2).
pub fn speak(id: u64, text: &str, voice: &str, rate: f32) {
    match engine() {
        Some(EngineKind::StandIn) => {
            if let Some(said) = STAND_IN.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                said.push(text.to_string());
            }
            set_stand_in_paused(false);
            dioxus::core::spawn_forever(async move {
                tokio::time::sleep(std::time::Duration::from_millis(60)).await;
                while stand_in_paused() {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                tell(Spoken::Ended(id));
            });
        }
        Some(EngineKind::Web) => {
            let arg = |s: &str| serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into());
            web_call(format!(
                "window.folitectaSpeech.speak({id}, {}, {}, {});",
                arg(text),
                arg(voice),
                rate.clamp(0.5, 2.0)
            ));
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        Some(EngineKind::SpeechDispatcher) => ssip::speak(id, text, voice, rate),
        _ => tell(Spoken::Failed(id, "no speech engine".into())),
    }
}

/// The engine pauses an utterance and goes on where it stopped; otherwise
/// (speech-dispatcher) a paused utterance is cut and spoken again.
pub fn resumes_in_place() -> bool {
    !matches!(engine(), Some(EngineKind::SpeechDispatcher))
}

/// Let the engine go when nothing more is to be read (speech-dispatcher's
/// connection is closed).
pub fn close() {
    #[cfg(all(unix, not(target_os = "macos")))]
    if engine() == Some(EngineKind::SpeechDispatcher) {
        ssip::close();
    }
}

/// Pause, go on, or stop the utterance being spoken.
pub fn pause() {
    match engine() {
        Some(EngineKind::StandIn) => set_stand_in_paused(true),
        Some(EngineKind::Web) => web_call("window.folitectaSpeech.pause();".into()),
        #[cfg(all(unix, not(target_os = "macos")))]
        Some(EngineKind::SpeechDispatcher) => ssip::cancel(),
        _ => {}
    }
}

pub fn resume() {
    match engine() {
        Some(EngineKind::StandIn) => set_stand_in_paused(false),
        Some(EngineKind::Web) => web_call("window.folitectaSpeech.resume();".into()),
        _ => {}
    }
}

pub fn cancel() {
    match engine() {
        Some(EngineKind::Web) => web_call("window.folitectaSpeech.cancel();".into()),
        #[cfg(all(unix, not(target_os = "macos")))]
        Some(EngineKind::SpeechDispatcher) => ssip::cancel(),
        _ => {}
    }
}
