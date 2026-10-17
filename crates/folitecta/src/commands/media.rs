//! Sound and video comments: adding them, the media player, playback
//! commands, properties and saving the media.
//!
//! The player gets its media from the `/media/{id}` asset path, served from
//! memory with byte ranges so that it can seek.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use dioxus::prelude::*;

use super::{handlers, no_doc, option_on, select_tool};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind, Tool};

/// Media type and bytes of media served to the player.
type Media = (String, Arc<Vec<u8>>);

/// Media served to the player, by id.
pub static MEDIA_STORE: LazyLock<Mutex<HashMap<u64, Media>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// What the player plays.
#[derive(Debug, Clone, PartialEq)]
pub struct Player {
    pub id: u64,
    pub tab: u64,
    pub page: usize,
    pub index: usize,
    pub kind: String,
    pub name: String,
    pub mime: String,
    /// Larger, in the middle of the window.
    pub floating: bool,
}

pub static PLAYER: GlobalSignal<Option<Player>> = Signal::global(|| None);

/// Answer a request of the player for `/media/{id}`, with byte ranges.
pub fn serve(path: &str, range: Option<&str>) -> (u16, Vec<(&'static str, String)>, Vec<u8>) {
    let id = path
        .trim_start_matches('/')
        .trim_start_matches("media/")
        .split(['/', '.'])
        .next()
        .and_then(|s| s.parse::<u64>().ok());
    let entry = id.and_then(|id| MEDIA_STORE.lock().ok()?.get(&id).cloned());
    let Some((mime, data)) = entry else {
        return (404, Vec::new(), Vec::new());
    };
    let len = data.len();
    let mut headers = vec![("Content-Type", mime), ("Accept-Ranges", "bytes".to_string())];
    // "bytes=start-end", either end open.
    let wanted = range.and_then(|r| r.strip_prefix("bytes=")).and_then(|r| {
        let (a, b) = r.split_once('-')?;
        let (start, end) = match (a.trim().parse::<usize>().ok(), b.trim().parse::<usize>().ok()) {
            (Some(s), Some(e)) => (s, e.min(len.saturating_sub(1))),
            (Some(s), None) => (s, len.saturating_sub(1)),
            (None, Some(n)) => (len.saturating_sub(n), len.saturating_sub(1)),
            (None, None) => return None,
        };
        (start <= end && start < len).then_some((start, end))
    });
    match wanted {
        Some((s, e)) => {
            headers.push(("Content-Range", format!("bytes {s}-{e}/{len}")));
            (206, headers, data[s..=e].to_vec())
        }
        None => (200, headers, data.to_vec()),
    }
}

fn next_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Media comment selected on the page, or the one in the player.
fn target(st: AppState) -> Option<(usize, usize)> {
    st.read_active(|t| t.selected_annot)
        .flatten()
        .filter(|(p, i)| {
            st.read_active(|t| t.doc.media_data(*p, *i).is_some())
                .unwrap_or(false)
        })
        .or_else(|| PLAYER.peek().as_ref().map(|p| (p.page, p.index)))
}

fn need_target(st: AppState) -> Option<(usize, usize)> {
    if no_doc(st) {
        return None;
    }
    let t = target(st);
    if t.is_none() {
        st.toast(
            ToastKind::Info,
            crate::i18n::t(st.lang(), "toast.select_media"),
            "",
        );
    }
    t
}

/// Open the player on a media comment and start it.
pub fn play_item(st: AppState, page: usize, index: usize) -> bool {
    if !option_on(st, "media.enabled") {
        st.toast(ToastKind::Info, t(st.lang(), "toast.media_disabled"), "");
        return true;
    }
    let Some((name, mime, data)) = st.read_active(|t| t.doc.media_data(page, index)).flatten() else {
        return false;
    };
    let kind = st
        .read_active(|t| {
            t.doc
                .media_items()
                .into_iter()
                .find(|m| m.page == page && m.index == index)
                .map(|m| m.kind)
        })
        .flatten()
        .unwrap_or_else(|| "audio".into());
    let id = next_id();
    if let Ok(mut store) = MEDIA_STORE.lock() {
        // Only the media of the player stays in memory.
        store.clear();
        store.insert(id, (mime.clone(), Arc::new(data)));
    }
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    let floating = PLAYER.peek().as_ref().is_some_and(|p| p.floating);
    *PLAYER.write() = Some(Player {
        id,
        tab,
        page,
        index,
        kind,
        name,
        mime,
        floating,
    });
    st.with_active(|t| t.selected_annot = Some((page, index)));
    control(st, "play");
    true
}

/// Run an action on the player element: `play`, `pause`, `toggle`, `stop`,
/// `mute`, `controls`, `fullscreen`, `exitfullscreen`, `volume`, `position`.
pub fn control(st: AppState, action: &str) {
    if PLAYER.peek().is_none() {
        match target(st) {
            Some((p, i)) if action == "play" || action == "toggle" => {
                play_item(st, p, i);
            }
            _ => st.toast(ToastKind::Info, t(st.lang(), "toast.select_media"), ""),
        }
        return;
    }
    let body = match action {
        "play" => "m.play().catch(() => {});",
        "pause" => "m.pause();",
        "toggle" => "if (m.paused) m.play().catch(() => {}); else m.pause();",
        "stop" => "m.pause(); m.currentTime = 0;",
        "mute" => "m.muted = !m.muted;",
        "controls" => "m.controls = !m.controls;",
        "fullscreen" => "if (m.requestFullscreen) m.requestFullscreen().catch(() => {});",
        "exitfullscreen" => "if (document.fullscreenElement) document.exitFullscreen().catch(() => {});",
        "volume" => "document.getElementById('folitecta-media-volume')?.focus();",
        "position" => "document.getElementById('folitecta-media-position')?.focus();",
        _ => "",
    };
    let js = format!(
        "for (let i = 0; i < 40 && !document.getElementById('folitecta-media'); i++) await new Promise(r => setTimeout(r, 30)); const m = document.getElementById('folitecta-media'); if (m) {{ {body} }}"
    );
    spawn(async move {
        let _ = document::eval(&js).await;
    });
}

pub fn close_player() {
    *PLAYER.write() = None;
    if let Ok(mut store) = MEDIA_STORE.lock() {
        store.clear();
    }
}

fn set_floating(on: bool) {
    if let Some(p) = PLAYER.write().as_mut() {
        p.floating = on;
    }
}

/// Save the media of a comment as a file (sampled sound as WAV).
fn save_media(st: AppState, want_video: Option<bool>) {
    let Some((p, i)) = need_target(st) else { return };
    let Some((name, mime, data)) = st.read_active(|t| t.doc.media_data(p, i)).flatten() else {
        return;
    };
    if let Some(video) = want_video {
        if video != mime.starts_with("video/") {
            st.toast(
                ToastKind::Info,
                t(
                    st.lang(),
                    if video {
                        "toast.not_video"
                    } else {
                        "toast.not_audio"
                    },
                ),
                "",
            );
            return;
        }
    }
    spawn(async move {
        let ext = name.rsplit('.').next().unwrap_or("bin").to_string();
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter(ext.to_uppercase(), &[ext.as_str()])
            .set_file_name(&name)
            .save_file()
            .await
        else {
            return;
        };
        match std::fs::write(file.path(), data) {
            Ok(()) => st.toast(
                ToastKind::Ok,
                tf(
                    st.lang(),
                    "toast.saved",
                    &[("name", &file.path().display().to_string())],
                ),
                "",
            ),
            Err(e) => st.error(e),
        }
    });
}

/// Add a sound comment at `at` (PDF space) from a WAV file chosen by the user.
pub fn pick_sound(st: AppState, page: usize, at: (f32, f32)) {
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("WAV", &["wav"])
            .pick_file()
            .await
        else {
            return;
        };
        let data = file.read().await;
        let author = st.ui.peek().author.clone();
        let r = st.with_active(|t| t.doc.add_sound(page, at, &data, &author).map(|_| ()));
        super::core_result(st, r);
        st.set_tool(Tool::Hand);
    });
}

/// Add a video or audio comment inside `rect` (PDF space) from a file chosen by the user.
pub fn pick_media(st: AppState, page: usize, rect: [f32; 4]) {
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter(
                t(st.lang(), "media.files"),
                &[
                    "mp4", "m4v", "webm", "ogv", "mov", "mp3", "m4a", "aac", "ogg", "oga", "wav", "flac",
                ],
            )
            .pick_file()
            .await
        else {
            return;
        };
        let name = file.file_name();
        let data = file.read().await;
        let author = st.ui.peek().author.clone();
        let r = st.with_active(|t| t.doc.add_media(page, rect, &name, data, &author).map(|_| ()));
        super::core_result(st, r);
        st.set_tool(Tool::Hand);
    });
}

fn properties(st: AppState) {
    if let Some((p, i)) = need_target(st) {
        st.set_dialog(Dialog::MediaProps { page: p, index: i });
    }
}

fn enable(st: AppState, on: bool) {
    st.set_option("media.enabled", on);
    if !on {
        close_player();
    }
    st.toast(
        ToastKind::Info,
        t(
            st.lang(),
            if on {
                "toast.media_enabled"
            } else {
                "toast.media_disabled"
            },
        ),
        "",
    );
}

/// Sound comments only for the sound commands.
fn sound(st: AppState, action: &str) {
    let is_sound = target(st)
        .and_then(|(p, i)| {
            st.read_active(|t| {
                t.doc
                    .media_items()
                    .into_iter()
                    .any(|m| m.page == p && m.index == i && m.kind == "sound")
            })
        })
        .unwrap_or(false);
    if !is_sound && PLAYER.peek().as_ref().is_none_or(|p| p.kind != "sound") {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_sound"), "");
        return;
    }
    control(st, action);
}

/// Check states of the media commands.
pub fn media_state(st: AppState, id: &str) -> Option<bool> {
    match id {
        "tool.annot.sound" | "tool.annot.editSound" => Some((st.tool)() == Tool::Sound),
        "tool.annot.editVideo" => Some((st.tool)() == Tool::Video),
        "mediaAnnot.startViewFloating" => Some(PLAYER.read().as_ref().is_some_and(|p| p.floating)),
        "mediaAnnot.enableContent" => Some(option_on(st, "media.enabled")),
        "mediaAnnot.disableContent" => Some(!option_on(st, "media.enabled")),
        _ => None,
    }
}

handlers! {
    "tool.annot.sound" => |st| select_tool(st, Tool::Sound),
    "tool.annot.editSound" => |st| select_tool(st, Tool::Sound),
    "tool.annot.editVideo" => |st| select_tool(st, Tool::Video),
    "tools.media" => |st| if !no_doc(st) { st.set_dialog(Dialog::MediaList) },
    "media.list" => |st| if !no_doc(st) { st.set_dialog(Dialog::MediaList) },
    "media.config" => properties,
    "media.play" => |st| control(st, "play"),
    "media.pause" => |st| control(st, "pause"),
    "media.play_pause" => |st| control(st, "toggle"),
    "media.stop" => |st| control(st, "stop"),
    "media.mute" => |st| control(st, "mute"),
    "media.contol" => |st| control(st, "controls"),
    "media.showControls" => |st| control(st, "play"),
    "media.position.Slider" => |st| control(st, "position"),
    "media.volume.Slider" => |st| control(st, "volume"),
    "volume.Slider" => |st| control(st, "volume"),
    "mediaAnnot.properties" => properties,
    "mediaAnnot.saveAudio" => |st| save_media(st, Some(false)),
    "mediaAnnot.saveVideo" => |st| save_media(st, Some(true)),
    "mediaAnnot.startFullScreen" => |st| control(st, "fullscreen"),
    "mediaAnnot.stopFullScreen" => |st| control(st, "exitfullscreen"),
    "mediaAnnot.startViewFloating" => |_| set_floating(true),
    "mediaAnnot.stopViewFloating" => |_| set_floating(false),
    "mediaAnnot.enableContent" => |st| enable(st, true),
    "mediaAnnot.disableContent" => |st| enable(st, false),
    "sound.play" => |st| sound(st, "play"),
    "sound.pause" => |st| sound(st, "pause"),
    "sound.stop" => |st| sound(st, "stop"),
    "sound.mute" => |st| sound(st, "mute"),
    "sound.saveAudio" => |st| save_media(st, Some(false)),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_of_the_media() {
        MEDIA_STORE
            .lock()
            .unwrap()
            .insert(9_999, ("audio/wav".into(), Arc::new((0u8..100).collect())));
        let (status, _, body) = serve("/media/9999", None);
        assert_eq!((status, body.len()), (200, 100));
        let (status, headers, body) = serve("/media/9999", Some("bytes=10-19"));
        assert_eq!(status, 206);
        assert_eq!(body, (10u8..20).collect::<Vec<u8>>());
        assert!(
            headers
                .iter()
                .any(|(k, v)| *k == "Content-Range" && v == "bytes 10-19/100")
        );
        let (_, _, tail) = serve("/media/9999", Some("bytes=-5"));
        assert_eq!(tail, vec![95, 96, 97, 98, 99]);
        let (_, _, open) = serve("/media/9999", Some("bytes=98-"));
        assert_eq!(open, vec![98, 99]);
        assert_eq!(serve("/media/12345", None).0, 404);
        MEDIA_STORE.lock().unwrap().remove(&9_999);
    }
}
