//! Saved preferences of the interface: theme, options and values, history of opened files and sessions.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPrefs {
    pub theme: Theme,
    pub lang: Lang,
    pub ribbon: bool,
    pub author: String,
    pub show_left_pane: bool,
    pub show_right_pane: bool,
    pub left_pane: String,
    pub recent: Vec<PathBuf>,
    /// On/off view options keyed by command id (rulers, grid, dark pages, ...).
    pub options: std::collections::BTreeMap<String, bool>,
    /// Valued settings keyed by name (ruler units, thumbnail size, ...).
    pub values: std::collections::BTreeMap<String, String>,
    /// Documents opened before, newest first.
    pub history: Vec<HistoryEntry>,
    /// Saved sets of open documents.
    pub sessions: Vec<Session>,
}

/// A document in the history of opened files.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryEntry {
    pub path: PathBuf,
    /// Document title (empty when it has none).
    pub title: String,
    pub pages: usize,
    /// When it was last opened and closed (seconds since 1970).
    pub opened: u64,
    pub closed: u64,
    /// Page shown when it was closed.
    pub page: usize,
    pub pinned: bool,
}

/// A saved set of documents.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub name: String,
    pub created: u64,
    pub opened: u64,
    pub files: Vec<SessionFile>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionFile {
    pub path: PathBuf,
    pub page: usize,
}

/// Seconds since 1970.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Default for UiPrefs {
    fn default() -> Self {
        UiPrefs {
            theme: Theme::Light,
            lang: Lang::En,
            ribbon: true,
            author: std::env::var("USERNAME")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_else(|_| "Author".into()),
            show_left_pane: true,
            show_right_pane: false,
            left_pane: "thumbnails".into(),
            recent: Vec::new(),
            options: Default::default(),
            values: Default::default(),
            history: Vec::new(),
            sessions: Vec::new(),
        }
    }
}

/// Per-user configuration directory without extra dependencies.
/// `FOLITECTA_CONFIG_DIR` overrides it (used by the interface self-test).
/// Folder of Folitecta's own data (settings, digital IDs, trusted certificates).
pub fn app_data_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("Folitecta"))
}

pub(super) fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("FOLITECTA_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library").join("Application Support"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    }
}

impl UiPrefs {
    fn path() -> Option<PathBuf> {
        config_dir().map(|d| d.join("Folitecta").join("settings.json"))
    }

    pub fn load() -> UiPrefs {
        let prefs: UiPrefs = Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let inherit = prefs
            .options
            .get("layersView.overrideByParent")
            .copied()
            .unwrap_or(false);
        LAYER_INHERIT.store(inherit, std::sync::atomic::Ordering::Relaxed);
        prefs
    }

    pub fn save(&self) {
        if let Some(path) = Self::path() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(json) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    pub fn value(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    pub fn push_recent(&mut self, path: &std::path::Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(12);
    }
}
