//! Hardware acceleration of the window's web view.
//!
//! The web views of Windows (WebView2) and macOS (WKWebView) composite on the
//! graphics card by themselves. On Linux, WebKitGTK does when its settings
//! allow it and its renderer can share buffers with the windowing system:
//! the window toolkit turned the shared-buffer renderer off on every Wayland
//! session (for the drivers where it shows blank windows), which left the
//! pages to be composited by the processor. Folitecta keeps it off only where
//! it is needed:
//!
//! - `FOLITECTA_GPU=off` (or `0`): no hardware acceleration at all;
//! - `FOLITECTA_GPU=on` (or `1`): hardware acceleration, shared buffers on;
//! - `FOLITECTA_GPU=no-shared-buffers`: hardware acceleration without them;
//! - otherwise: on, except with the NVIDIA driver, where the shared buffers
//!   stay off (the known cause of blank windows).
//!
//! The window stays a window of the session's own system (Wayland or X11),
//! as before: the toolkit's request for X11 came after the window system had
//! been chosen and changed nothing; it is no longer made. What was chosen
//! and what the web process really loaded (graphics drivers) is reported by
//! [`report`], for the performance step of the self-test.

use std::sync::OnceLock;

/// The choice made at start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Acceleration {
    /// No hardware acceleration (asked by `FOLITECTA_GPU=off`).
    Off,
    /// Hardware acceleration, without the shared-buffer renderer.
    NoSharedBuffers,
    /// Hardware acceleration with the shared-buffer renderer.
    On,
}

static CHOICE: OnceLock<Acceleration> = OnceLock::new();
static SETTINGS: OnceLock<String> = OnceLock::new();

/// Decide before the window is made (the environment of the web process is
/// read when it starts).
pub fn choose() -> Acceleration {
    *CHOICE.get_or_init(|| {
        let asked = std::env::var("FOLITECTA_GPU")
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(asked.as_str(), "off" | "0" | "no" | "false") {
            return Acceleration::Off;
        }
        if asked == "no-shared-buffers" {
            return Acceleration::NoSharedBuffers;
        }
        if !cfg!(target_os = "linux") {
            return Acceleration::On;
        }
        let nvidia = std::path::Path::new("/proc/driver/nvidia").exists();
        if nvidia && !matches!(asked.as_str(), "on" | "1" | "yes" | "true") {
            return Acceleration::NoSharedBuffers;
        }
        Acceleration::On
    })
}

/// Set the environment for the choice (Linux), before any thread starts.
pub fn prepare_environment() {
    #[cfg(target_os = "linux")]
    {
        match choose() {
            Acceleration::On => {}
            Acceleration::NoSharedBuffers | Acceleration::Off => {
                // SAFETY: called at the start of main, before any other thread.
                unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
            }
        }
    }
}

/// Apply the choice to the web view's settings, once the window exists.
pub fn apply(window: &dioxus::desktop::DesktopContext) {
    #[cfg(target_os = "linux")]
    {
        use dioxus::desktop::wry::WebViewExtUnix;
        use webkit2gtk::{HardwareAccelerationPolicy, SettingsExt, WebViewExt};
        let view = window.webview.webview();
        if let Some(settings) = WebViewExt::settings(&view) {
            let policy = match choose() {
                Acceleration::Off => HardwareAccelerationPolicy::Never,
                _ => HardwareAccelerationPolicy::Always,
            };
            let before = settings.hardware_acceleration_policy();
            settings.set_hardware_acceleration_policy(policy);
            let _ = SETTINGS.set(format!(
                "policy {before:?} -> {:?}, webgl {}",
                settings.hardware_acceleration_policy(),
                settings.enables_webgl(),
            ));
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = window;
        let _ = SETTINGS.set("the web view composites on the graphics card by itself".into());
    }
}

/// The graphics libraries a process has loaded (Linux): drivers and GL.
#[cfg(target_os = "linux")]
fn graphics_libraries(pid: &str) -> Vec<String> {
    let Ok(maps) = std::fs::read_to_string(format!("/proc/{pid}/maps")) else {
        return Vec::new();
    };
    let mut out: Vec<String> = maps
        .lines()
        .filter_map(|l| l.split_whitespace().nth(5))
        .filter_map(|path| path.rsplit('/').next())
        .filter(|name| {
            let n = name.to_ascii_lowercase();
            n.contains("_dri")
                || n.starts_with("libgallium")
                || n.starts_with("libegl")
                || n.starts_with("libgl")
                || n.starts_with("libgbm")
                || n.starts_with("libvulkan")
                || n.starts_with("libdrm_")
                || n.contains("nvidia")
        })
        .map(str::to_string)
        .collect();
    out.sort();
    out.dedup();
    out
}

/// What was chosen, the web view's settings and, on Linux, the graphics
/// libraries the web processes of this program loaded.
pub fn report() -> serde_json::Value {
    let mut out = serde_json::json!({
        "choice": format!("{:?}", choose()),
        "settings": SETTINGS.get().cloned().unwrap_or_default(),
    });
    #[cfg(target_os = "linux")]
    {
        let me = std::process::id().to_string();
        let mut web = Vec::new();
        if let Ok(dir) = std::fs::read_dir("/proc") {
            for e in dir.flatten() {
                let pid = e.file_name().to_string_lossy().to_string();
                if !pid.chars().all(|c| c.is_ascii_digit()) {
                    continue;
                }
                let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else {
                    continue;
                };
                let field = |key: &str| {
                    status
                        .lines()
                        .find(|l| l.starts_with(key))
                        .map(|l| l[key.len()..].trim().to_string())
                };
                if !field("Name:").is_some_and(|n| n.starts_with("WebKitWebProces")) {
                    continue;
                }
                // One of ours: this process is among its parents.
                let mut parent = field("PPid:");
                for _ in 0..6 {
                    match parent {
                        Some(p) if p == me => {
                            web.push(graphics_libraries(&pid));
                            break;
                        }
                        Some(p) if p != "0" && p != "1" => {
                            parent = std::fs::read_to_string(format!("/proc/{p}/status"))
                                .ok()
                                .and_then(|s| {
                                    s.lines()
                                        .find(|l| l.starts_with("PPid:"))
                                        .map(|l| l[5..].trim().to_string())
                                });
                        }
                        _ => break,
                    }
                }
            }
        }
        out["web_process_graphics"] = serde_json::json!(web);
        out["ui_process_graphics"] = serde_json::json!(graphics_libraries("self"));
        for var in [
            "GDK_BACKEND",
            "WEBKIT_DISABLE_DMABUF_RENDERER",
            "WEBKIT_DISABLE_COMPOSITING_MODE",
            "XDG_SESSION_TYPE",
        ] {
            out[var] = serde_json::json!(std::env::var(var).ok());
        }
    }
    out
}
