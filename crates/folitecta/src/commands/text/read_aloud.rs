//! Reading aloud (FOL-L042): the paragraphs of the document from the current
//! page (or the text selected) spoken one after the other with a voice of
//! the system, the one being read marked on its page (and kept in view) or
//! in the reflow view. The bar of View › Read Aloud starts, pauses, goes
//! back and forth by paragraph and stops; the voice and the speed are kept
//! in the settings.

use dioxus::prelude::*;
use folitecta_core::ReadBlock;

use crate::commands::{handlers, no_doc, option_on};
use crate::speech::{self, EngineKind, Spoken, Voice};
use crate::state::AppState;

/// What is being read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReadState {
    pub running: bool,
    pub paused: bool,
    /// Looking for a voice.
    pub starting: bool,
    /// The tab read and its paragraphs, in order.
    pub tab: u64,
    pub blocks: Vec<ReadBlock>,
    /// The paragraph being read.
    pub current: usize,
    /// The utterance of the paragraph being read: the end of any other one
    /// (cut short, skipped) is not waited for.
    pub utterance: u64,
    /// The engine and its voices, once found.
    pub engine: Option<EngineKind>,
    pub voices: Vec<Voice>,
    /// Why nothing is read (no text, no voice, a voice that failed).
    pub problem: Option<String>,
}

pub static READ: GlobalSignal<ReadState> = Signal::global(ReadState::default);

/// The paragraph being read in the active tab: its page and first character.
pub fn current_block(st: AppState) -> Option<(usize, usize)> {
    let r = READ.read();
    let active = st.read_active(|t| t.id)?;
    (r.running && r.tab == active)
        .then(|| r.blocks.get(r.current).map(|b| (b.page, b.start)))
        .flatten()
}

/// The speed chosen (0.5 to 2, 1 normal).
pub fn rate(st: AppState) -> f32 {
    st.value("read.rate")
        .trim()
        .parse()
        .unwrap_or(1.0f32)
        .clamp(0.5, 2.0)
}

/// The voice chosen (an id of the engine's voices; empty: its own).
pub fn voice(st: AppState) -> String {
    st.value("read.voice")
}

/// Follow the ends of utterances for as long as the window lives.
pub fn follow(st: AppState) {
    let Some(mut rx) = speech::events() else { return };
    dioxus::core::spawn_forever(async move {
        while let Some(ev) = rx.recv().await {
            let (id, failed) = match ev {
                Spoken::Ended(id) => (id, None),
                Spoken::Failed(id, why) => (id, Some(why)),
            };
            let current = {
                let r = READ.peek();
                (r.running && r.utterance == id).then_some(r.current)
            };
            let Some(current) = current else { continue };
            match failed {
                // A voice that fails stops the reading, saying why.
                Some(why) => {
                    stop(st);
                    READ.write().problem = Some(why);
                }
                None => go_to(st, current + 1),
            }
        }
    });
}

/// The paragraphs to read: the text selected, else the paragraphs from the
/// current page to the end.
fn blocks_to_read(st: AppState) -> Option<(u64, Vec<ReadBlock>)> {
    st.read_active(|t| {
        let n = t.doc.page_count();
        if let Some(sel) = t.text_sel {
            let (a, b) = (sel.anchor.min(sel.head), sel.anchor.max(sel.head));
            let chars: Vec<char> = t
                .doc
                .text_layer(sel.page)
                .map(|l| l.text.chars().collect())
                .unwrap_or_default();
            let text: String = chars
                .get(a.min(chars.len())..b.min(chars.len()))
                .map(|c| c.iter().collect())
                .unwrap_or_default();
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if !text.is_empty() {
                return (
                    t.id,
                    vec![ReadBlock {
                        page: sel.page,
                        start: a,
                        end: b,
                        text,
                        level: 0,
                    }],
                );
            }
        }
        (t.id, t.doc.reading_blocks(t.page.min(n)..n))
    })
}

/// What to say when no engine has a voice (on Linux: speech-dispatcher speaks).
fn no_voice_key() -> &'static str {
    if cfg!(all(unix, not(target_os = "macos"))) {
        "read.no_voice_linux"
    } else {
        "read.no_voice"
    }
}

/// Start reading, or go on after a pause.
pub fn start(st: AppState) {
    if no_doc(st) {
        return;
    }
    let (running, paused, starting) = {
        let r = READ.peek();
        (r.running, r.paused, r.starting)
    };
    if running && paused {
        go_on(st);
        return;
    }
    if running || starting {
        return;
    }
    let Some((tab, blocks)) = blocks_to_read(st) else {
        return;
    };
    if blocks.is_empty() {
        READ.write().problem = Some(crate::i18n::t(st.lang(), "read.no_text"));
        return;
    }
    {
        let mut r = READ.write();
        r.starting = true;
        r.problem = None;
    }
    spawn(async move {
        let chosen = speech::choose().await;
        {
            let mut r = READ.write();
            r.starting = false;
            let Some((kind, voices)) = chosen else {
                r.problem = Some(crate::i18n::t(st.lang(), no_voice_key()));
                return;
            };
            r.engine = Some(kind);
            r.voices = voices;
            r.tab = tab;
            r.blocks = blocks;
            r.running = true;
            r.paused = false;
        }
        go_to(st, 0);
    });
}

/// Read paragraph `i` (or end the reading after the last one).
pub fn go_to(st: AppState, i: usize) {
    let (block, id) = {
        let mut r = READ.write();
        if !r.running {
            return;
        }
        let Some(b) = r.blocks.get(i).cloned() else {
            drop(r);
            stop(st);
            return;
        };
        r.current = i;
        r.utterance += 1;
        r.paused = false;
        (b, r.utterance)
    };
    mark(st, &block);
    speech::speak(id, &block.text, &voice(st), rate(st));
}

/// Mark the paragraph read on its page and bring it into view (the reflow
/// view keeps it in view itself).
fn mark(st: AppState, b: &ReadBlock) {
    let tab = READ.peek().tab;
    if st.read_active(|t| t.id) != Some(tab) {
        return;
    }
    let (rects, height) = st
        .read_active(|t| {
            (
                t.doc
                    .text_layer(b.page)
                    .map(|l| l.range_rects(b.start, b.end))
                    .unwrap_or_default(),
                t.page_sizes.get(b.page).map(|s| s.height).unwrap_or(842.0),
            )
        })
        .unwrap_or_default();
    let page = b.page;
    let top = rects.first().map(|r| r[1]);
    st.with_active(|t| t.reading = Some((page, rects)));
    if option_on(st, "view.reflow") {
        return;
    }
    if st.current_page() != page {
        st.navigate(page, false);
    }
    if let Some(top) = top {
        // The first line in the upper third of the view, once the page is drawn.
        let js = format!(
            "setTimeout(() => {{ const p = document.getElementById('pg-{page}'); const c = document.getElementById('canvas'); if (!p || !c) return; const y = p.getBoundingClientRect().top - c.getBoundingClientRect().top + {top} * p.getBoundingClientRect().height / {height}; if (y < c.clientHeight * 0.15 || y > c.clientHeight * 0.75) c.scrollBy({{ top: y - c.clientHeight / 3, behavior: 'smooth' }}); }}, 80);",
            height = height.max(1.0)
        );
        spawn(async move {
            let _ = document::eval(&js).await;
        });
    }
}

/// Pause the reading, or go on (or start it).
pub fn pause(st: AppState) {
    let (running, paused) = {
        let r = READ.peek();
        (r.running, r.paused)
    };
    if !running {
        start(st);
        return;
    }
    if paused {
        go_on(st);
        return;
    }
    {
        let mut r = READ.write();
        r.paused = true;
        // An engine that cannot go on where it stopped cuts the paragraph:
        // that end is not waited for, the paragraph is spoken again.
        if !speech::resumes_in_place() {
            r.utterance += 1;
        }
    }
    speech::pause();
}

/// Go on after a pause.
fn go_on(st: AppState) {
    if speech::resumes_in_place() {
        READ.write().paused = false;
        speech::resume();
    } else {
        let current = READ.peek().current;
        go_to(st, current);
    }
}

/// The paragraph before (`-1`) or after (`1`) the one read.
pub fn step(st: AppState, delta: isize) {
    let (running, current, n) = {
        let r = READ.peek();
        (r.running, r.current, r.blocks.len())
    };
    if !running || n == 0 {
        return;
    }
    let next = (current as isize + delta).clamp(0, n as isize - 1) as usize;
    // The utterance under way is cut; its end is not waited for.
    READ.write().utterance += 1;
    speech::cancel();
    go_to(st, next);
}

/// Stop reading; the mark of the paragraph goes.
pub fn stop(st: AppState) {
    let was = {
        let mut r = READ.write();
        let was = r.running || r.starting;
        r.running = false;
        r.paused = false;
        r.starting = false;
        r.utterance += 1;
        was.then_some(r.tab)
    };
    speech::cancel();
    if let Some(tab) = was {
        let mut docs = st.docs_mut();
        if let Some(t) = docs.iter_mut().find(|t| t.id == tab) {
            t.reading = None;
        }
    }
}

/// Check states of the reflow view and of the reading bar (settings of
/// their own, not view options: their commands do more than switch them).
pub fn reading_state(st: AppState, id: &str) -> Option<bool> {
    matches!(id, "view.reflow" | "view.readAloud").then(|| option_on(st, id))
}

/// The bar of View › Read Aloud on or off; off, the reading stops.
fn toggle(st: AppState) {
    if no_doc(st) {
        return;
    }
    let on = !option_on(st, "view.readAloud");
    st.set_option("view.readAloud", on);
    if !on {
        stop(st);
        speech::close();
    }
}

handlers! {
    "view.readAloud" => toggle,
}
