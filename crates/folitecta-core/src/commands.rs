//! The command registry: every user-facing command, its labels, placement in
//! the classic menus, ribbon and panes, keyboard shortcut and implementation status.
//!
//! The data lives in `assets/commands.json`; this module loads it once and offers
//! query helpers used by the user interface and by the documentation generator.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

const DATA: &str = include_str!("../assets/commands.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandKind {
    Command,
    Submenu,
    Toggle,
    Slider,
    Combo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Planned,
    Partial,
    Implemented,
}

impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Status::Planned => "Planned",
            Status::Partial => "Partial",
            Status::Implemented => "Implemented",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RibbonSlot {
    pub tab: String,
    pub group: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub id: String,
    pub label: String,
    pub parent: Option<String>,
    pub menu: Option<String>,
    pub ribbon: Option<RibbonSlot>,
    pub pane: Option<String>,
    pub kind: CommandKind,
    pub shortcut: Option<String>,
    pub status: Status,
}

#[derive(Debug, Deserialize)]
struct RawRegistry {
    version: u32,
    commands: Vec<Command>,
}

/// The loaded registry with lookup indexes.
pub struct Registry {
    pub version: u32,
    commands: Vec<Command>,
    by_id: HashMap<String, usize>,
    children: HashMap<String, Vec<usize>>,
}

/// Classic menu bar, in order.
pub const MENUS: &[(&str, &str)] = &[
    ("file", "File"),
    ("edit", "Edit"),
    ("view", "View"),
    ("document", "Document"),
    ("comments", "Comments"),
    ("form", "Form"),
    ("tools", "Tools"),
    ("window", "Window"),
    ("help", "Help"),
];

/// Main ribbon tabs, in order.
pub const RIBBON_TABS: &[(&str, &str)] = &[
    // The first tab (id "home"): the tools and editing of the pages.
    ("home", "Edit"),
    ("view", "View"),
    ("comment", "Comment"),
    ("protect", "Protect"),
    ("form", "Form"),
    ("organize", "Organize"),
    ("convert", "Convert"),
    ("share", "Share"),
    ("review", "Review"),
    ("acc", "Accessibility"),
    ("help", "Help"),
];

/// Contextual ribbon tabs (shown when the matching tool or selection is active).
pub const CONTEXT_TABS: &[(&str, &str)] = &[
    ("ctx.arrange", "Arrange"),
    ("ctx.format", "Format"),
    ("ctx.content", "Content"),
    ("ctx.path", "Edit Path"),
    ("ctx.portfolio", "Portfolio"),
    ("ctx.tool", "Tool"),
    ("ctx.stamp", "Stamp"),
    ("ctx.calculator", "Calculator"),
];

/// Ribbon groups `(tab, group, label)` in display order.
pub const RIBBON_GROUPS: &[(&str, &str, &str)] = &[
    ("home", "tools", "Tools"),
    ("home", "text", "Text"),
    ("home", "edit", "Clipboard"),
    ("home", "objects", "Objects"),
    ("home", "comment", "Comment"),
    ("home", "view", "View"),
    ("home", "links", "Links"),
    ("home", "protect", "Protect"),
    ("view", "tools", "Tools"),
    ("view", "goto", "Go To"),
    ("view", "zoom", "Zoom"),
    ("view", "pageDisplay", "Page Display"),
    ("view", "window", "Window"),
    ("comment", "tools", "Tools"),
    ("comment", "text", "Text"),
    ("comment", "note", "Note"),
    ("comment", "textMarkup", "Text Markup"),
    ("comment", "drawing", "Drawing"),
    ("comment", "measure", "Measure"),
    ("comment", "manage", "Manage Comments"),
    ("comment", "popup", "Popup"),
    ("comment", "collaboration", "Collaboration"),
    ("comment", "media", "Media"),
    ("comment", "soundControl", "Sound"),
    ("protect", "digiSigns", "Digital Signatures"),
    ("protect", "signsAndInitials", "Signatures and Initials"),
    ("protect", "redact", "Redact"),
    ("protect", "docSecurity", "Document Security"),
    ("protect", "esign", "E-Signature"),
    ("form", "fields", "Form Fields"),
    ("form", "data", "Form Data"),
    ("form", "javaScript", "JavaScript"),
    ("organize", "pages", "Pages"),
    ("organize", "transformPages", "Transform Pages"),
    ("organize", "pageMarks", "Page Marks"),
    ("convert", "create", "Create"),
    ("convert", "export", "Export"),
    ("convert", "modify", "Modify"),
    ("convert", "ocr", "OCR"),
    ("convert", "enhancePages", "Enhance Pages"),
    ("share", "sendTo", "Send To"),
    ("review", "proofing", "Proofing"),
    ("review", "comments", "Comments"),
    ("acc", "autotag", "Auto-Tag"),
    ("acc", "check", "Check"),
    ("acc", "edit", "Edit"),
    ("help", "ui", "UI Options"),
    ("help", "help", "Help"),
    ("help", "product", "Product"),
    ("ctx.arrange", "align", "Align"),
    ("ctx.arrange", "distribute", "Distribute"),
    ("ctx.arrange", "size", "Size"),
    ("ctx.arrange", "rotate", "Rotate"),
    ("ctx.arrange", "reflect", "Reflect"),
    ("ctx.arrange", "zOrder", "Z-Order"),
    ("ctx.arrange", "transform", "Transform"),
    ("ctx.format", "shapeStyle", "Shape Style"),
    ("ctx.format", "textFormat", "Text Format"),
    ("ctx.content", "tools", "Content Tools"),
    ("ctx.path", "edit", "Path Editing"),
    ("ctx.portfolio", "view", "Portfolio"),
    ("ctx.tool", "options", "Tool Options"),
    ("ctx.stamp", "designer", "Stamp Designer"),
    ("ctx.calculator", "tape", "Accounting Calculator"),
];

/// Side panes `(id, label)`.
pub const PANES: &[(&str, &str)] = &[
    ("thumbnails", "Thumbnails"),
    ("bookmarks", "Bookmarks"),
    ("comments", "Comments"),
    ("attachments", "Attachments"),
    ("layers", "Layers"),
    ("fields", "Fields"),
    ("links", "Links"),
    ("destinations", "Named Destinations"),
    ("search", "Search"),
    ("signatures", "Signatures"),
    ("content", "Content"),
    ("stamps", "Stamps"),
    ("history", "History"),
    ("sessions", "Sessions"),
    ("explorer", "Explorer"),
    ("tags", "Tags"),
    ("order", "Order"),
    ("accessibility-checker", "Accessibility Checker"),
    ("accessibility-report", "Accessibility Report"),
    ("3d-model-tree", "3D Model Tree"),
    ("assistant", "Assistant"),
    ("comparison", "Comparison"),
    ("loupe", "Loupe"),
    ("pan-zoom", "Pan and Zoom"),
    ("javascript-console", "JavaScript Console"),
    ("properties", "Properties"),
    ("comment-styles", "Comment Styles"),
    ("recovery", "Recovery"),
];

/// Preferred order of the first entries of each classic menu. Entries not listed
/// follow in alphabetical order. The literal `"-"` inserts a separator.
pub const MENU_ORDER: &[(&str, &[&str])] = &[
    (
        "file",
        &[
            "open",
            "openFrom",
            "recentFiles",
            "sessions",
            "-",
            "newDoc",
            "newPortfolio",
            "-",
            "save",
            "saveAs",
            "saveAsCopy",
            "saveAll",
            "revert",
            "-",
            "close",
            "closeAll",
            "-",
            "export",
            "document.export",
            "sendByEmail",
            "shareGroup",
            "-",
            "print",
            "printAll",
            "-",
            "docProps",
            "document.security",
            "-",
            "exit",
        ],
    ),
    (
        "edit",
        &[
            "undo",
            "redo",
            "-",
            "cut",
            "copy",
            "paste",
            "pasteSpecial",
            "delete",
            "-",
            "selectAll",
            "invertSelection",
            "edit.deselect",
            "-",
            "find",
            "findAndRepl",
            "search",
            "-",
            "tool.editText",
            "pageText.insert",
            "pageText.replace",
            "pageText.delete",
            "-",
            "arrange",
            "object",
            "-",
            "prefs",
            "settings",
        ],
    ),
    (
        "view",
        &[
            "view.goto",
            "view.zoom",
            "zoomIn",
            "zoomOut",
            "view.layout",
            "view.rotate",
            "view.fullScreen",
            "-",
            "view.pageThumbnails",
            "view.bookmarks",
            "view.comments",
            "view.attachments",
            "view.layers",
            "view.fields",
            "view.links",
            "view.destinations",
            "view.signatures",
            "view.search",
            "view.properties",
            "view.otherPanes",
            "-",
            "view.showRulers",
            "view.showGrid",
            "view.showGuides",
            "view.snap",
            "view.darkPageMode",
            "-",
            "view.toolbars",
            "view.theme",
            "view.uiOptions",
        ],
    ),
    (
        "document",
        &[
            "document.insertPages",
            "document.insertEmptyPages",
            "document.insertImages",
            "document.deletePages",
            "document.extractPages",
            "document.replacePages",
            "document.movePages",
            "document.duplicatePages",
            "document.swapPages",
            "document.reversePageOrder",
            "document.mergePages",
            "document.splitGroup",
            "-",
            "document.rotatePages",
            "document.cropPages",
            "document.resizePages",
            "document.setMargins",
            "-",
            "document.watermarks",
            "document.background",
            "document.headerAndFooter",
            "document.batesNumbering",
            "document.pageNumbering",
            "-",
            "document.OCRPages",
            "document.recompressImages",
            "document.replaceFonts",
            "-",
            "document.signatures",
            "securityPolicies",
            "document.redaction",
            "-",
            "document.javaScript",
            "document.actions",
            "document.wordCount",
            "docProps",
        ],
    ),
    (
        "comments",
        &[
            "addStickyNote",
            "tool.annot",
            "comments.newGroup",
            "-",
            "comments.show",
            "comments.next",
            "comments.previous",
            "-",
            "comments.deleteGroup",
            "comments.flattenGroup",
            "-",
            "comments.import",
            "comments.export",
            "comments.summarize",
            "-",
            "annotStylesList",
            "commentStyles",
        ],
    ),
    (
        "form",
        &[
            "tool.editFields",
            "forms.highlight",
            "forms.reset",
            "-",
            "forms.importData",
            "forms.exportData",
            "forms.emailData",
            "forms.manageData",
            "forms.calcOrder",
        ],
    ),
    (
        "tools",
        &[
            "tool.hand",
            "tool.selectText",
            "tool.snapshot",
            "tool.zoom",
            "tool.loupe",
            "tool.panZoom",
            "-",
            "tool.editContent",
            "tool.editText",
            "tool.addText",
            "tool.table",
            "-",
            "tools.basic",
            "tools.markup",
            "tools.measure",
            "tools.editContent",
            "tools.editLinks",
            "tools.media",
            "tools.zoom",
            "-",
            "compareDocs",
            "launchExternalApp",
        ],
    ),
    (
        "window",
        &[
            "window.new",
            "duplicateTab",
            "-",
            "closeTab",
            "closeOtherTabs",
            "closeTabsToTheLeft",
            "closeTabsToTheRight",
            "-",
            "window.split",
            "window.removeSplit",
        ],
    ),
    (
        "help",
        &[
            "help.contents",
            "help.cmdline",
            "help.homepage",
            "help.support",
            "help.report",
            "-",
            "help.checkForUpdates",
            "help.about",
        ],
    ),
];

static REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::load);

/// The global registry.
pub fn registry() -> &'static Registry {
    &REGISTRY
}

impl Registry {
    fn load() -> Self {
        let raw: RawRegistry = serde_json::from_str(DATA).expect("commands.json is valid");
        let mut by_id = HashMap::with_capacity(raw.commands.len());
        let mut children: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, c) in raw.commands.iter().enumerate() {
            by_id.insert(c.id.clone(), i);
            if let Some(p) = &c.parent {
                children.entry(p.clone()).or_default().push(i);
            }
        }
        Registry {
            version: raw.version,
            commands: raw.commands,
            by_id,
            children,
        }
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn all(&self) -> &[Command] {
        &self.commands
    }

    pub fn get(&self, id: &str) -> Option<&Command> {
        self.by_id.get(id).map(|i| &self.commands[*i])
    }

    /// Direct children of a submenu, sorted by label.
    pub fn children(&self, id: &str) -> Vec<&Command> {
        let mut out: Vec<&Command> = self
            .children
            .get(id)
            .map(|v| v.iter().map(|i| &self.commands[*i]).collect())
            .unwrap_or_default();
        out.sort_by(|a, b| a.label.cmp(&b.label));
        out
    }

    pub fn has_children(&self, id: &str) -> bool {
        self.children.get(id).map(|v| !v.is_empty()).unwrap_or(false)
    }

    fn is_root_title(id: &str) -> bool {
        MENUS.iter().any(|(m, _)| *m == id)
    }

    /// Top-level entries of a classic menu (ids and `"-"` separators), in display order.
    pub fn menu_entries(&self, menu: &str) -> Vec<Option<&Command>> {
        let mut top: Vec<&Command> = self
            .commands
            .iter()
            .filter(|c| c.menu.as_deref() == Some(menu) && !Self::is_root_title(&c.id))
            .filter(|c| match &c.parent {
                None => true,
                Some(p) => match self.get(p) {
                    None => true,
                    Some(parent) => Self::is_root_title(&parent.id) || parent.menu.as_deref() != Some(menu),
                },
            })
            .collect();
        top.sort_by(|a, b| a.label.cmp(&b.label));
        let order = MENU_ORDER
            .iter()
            .find(|(m, _)| *m == menu)
            .map(|(_, o)| *o)
            .unwrap_or(&[]);
        let mut out: Vec<Option<&Command>> = Vec::with_capacity(top.len() + 8);
        let mut used: HashSet<&str> = HashSet::new();
        for &id in order {
            if id == "-" {
                if !matches!(out.last(), Some(None) | None) {
                    out.push(None);
                }
                continue;
            }
            if let Some(c) = top.iter().find(|c| c.id == id) {
                out.push(Some(c));
                used.insert(id);
            }
        }
        let rest: Vec<&Command> = top
            .iter()
            .copied()
            .filter(|c| !used.contains(c.id.as_str()))
            .collect();
        if !rest.is_empty() && !out.is_empty() && !matches!(out.last(), Some(None)) {
            out.push(None);
        }
        out.extend(rest.into_iter().map(Some));
        while matches!(out.last(), Some(None)) {
            out.pop();
        }
        out
    }

    /// Commands shown as buttons in a ribbon group (children of a command in the
    /// same group are offered through the parent's dropdown).
    pub fn ribbon_group(&self, tab: &str, group: &str) -> Vec<&Command> {
        let in_group = |c: &Command| {
            c.ribbon
                .as_ref()
                .map(|r| r.tab == tab && r.group == group)
                .unwrap_or(false)
        };
        let mut out: Vec<&Command> = self
            .commands
            .iter()
            .filter(|c| in_group(c))
            .filter(|c| match &c.parent {
                Some(p) => !self.get(p).map(in_group).unwrap_or(false),
                None => true,
            })
            .collect();
        out.sort_by(|a, b| a.label.cmp(&b.label));
        out
    }

    /// Groups of a ribbon tab (ids and labels) in display order, only those with commands.
    pub fn ribbon_groups(&self, tab: &str) -> Vec<(&'static str, &'static str)> {
        RIBBON_GROUPS
            .iter()
            .filter(|(t, g, _)| {
                *t == tab
                    && self.commands.iter().any(|c| {
                        c.ribbon
                            .as_ref()
                            .map(|r| r.tab == *t && r.group == *g)
                            .unwrap_or(false)
                    })
            })
            .map(|(_, g, label)| (*g, *label))
            .collect()
    }

    /// Commands that belong to a pane's toolbar or context menu (top-level only).
    pub fn pane_commands(&self, pane: &str) -> Vec<&Command> {
        let mut out: Vec<&Command> = self
            .commands
            .iter()
            .filter(|c| c.pane.as_deref() == Some(pane))
            .filter(|c| match &c.parent {
                Some(p) => self
                    .get(p)
                    .map(|pc| pc.pane.as_deref() != Some(pane))
                    .unwrap_or(true),
                None => true,
            })
            .collect();
        out.sort_by(|a, b| a.label.cmp(&b.label));
        out
    }

    /// Full label path, e.g. `Document › Watermarks › Add…`.
    pub fn path_label(&self, id: &str) -> String {
        let mut parts = Vec::new();
        let mut current = self.get(id);
        let mut guard = 0;
        while let Some(c) = current {
            parts.push(c.label.trim_end_matches('…').to_string());
            current = c.parent.as_deref().and_then(|p| self.get(p));
            guard += 1;
            if guard > 16 {
                break;
            }
        }
        if let Some(menu) = self.get(id).and_then(|c| c.menu.as_deref()) {
            if let Some((_, label)) = MENUS.iter().find(|(m, _)| *m == menu) {
                if parts.last().map(|l| l != label).unwrap_or(true) {
                    parts.push(label.to_string());
                }
            }
        }
        parts.reverse();
        parts.join(" › ")
    }

    /// Case-insensitive search over labels, ids and paths; best matches first.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&Command> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(i32, &Command)> = self
            .commands
            .iter()
            .filter(|c| c.kind != CommandKind::Submenu || self.children(&c.id).is_empty())
            .filter_map(|c| {
                let label = c.label.trim_end_matches('…').to_lowercase();
                let id = c.id.to_lowercase();
                let path = self.path_label(&c.id).to_lowercase();
                let score = if label == q {
                    100
                } else if label.starts_with(&q) {
                    80
                } else if label.contains(&q) {
                    60
                } else if path.contains(&q) {
                    40
                } else if id.contains(&q) {
                    20
                } else {
                    return None;
                };
                Some((score, c))
            })
            .collect();
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.id.len().cmp(&b.1.id.len()))
                .then_with(|| a.1.label.cmp(&b.1.label))
        });
        scored.into_iter().take(limit).map(|(_, c)| c).collect()
    }

    /// Find a command by shortcut string (normalised, e.g. `Ctrl+Shift+S`).
    pub fn by_shortcut(&self, shortcut: &str) -> Option<&Command> {
        let s = shortcut.to_lowercase();
        self.commands.iter().find(|c| {
            c.shortcut
                .as_deref()
                .map(|k| k.to_lowercase() == s)
                .unwrap_or(false)
        })
    }

    /// Label of a ribbon tab.
    pub fn tab_label(tab: &str) -> &'static str {
        RIBBON_TABS
            .iter()
            .chain(CONTEXT_TABS.iter())
            .find(|(t, _)| *t == tab)
            .map(|(_, l)| *l)
            .unwrap_or("")
    }

    /// Counts per status given the set of implemented and partial ids.
    pub fn status_summary(
        &self,
        implemented: &HashSet<&str>,
        partial: &HashSet<&str>,
    ) -> (usize, usize, usize) {
        let mut done = 0;
        let mut part = 0;
        let mut planned = 0;
        for c in &self.commands {
            if implemented.contains(c.id.as_str()) {
                done += 1;
            } else if partial.contains(c.id.as_str()) {
                part += 1;
            } else {
                planned += 1;
            }
        }
        (done, part, planned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_loads_and_is_consistent() {
        let r = registry();
        assert!(r.len() > 1900, "registry has {} commands", r.len());
        let mut seen = HashSet::new();
        for c in r.all() {
            assert!(seen.insert(c.id.as_str()), "duplicate id {}", c.id);
            assert!(!c.label.trim().is_empty(), "empty label for {}", c.id);
            if let Some(p) = &c.parent {
                assert!(r.get(p).is_some(), "missing parent {p} for {}", c.id);
            }
            if let Some(m) = &c.menu {
                assert!(
                    MENUS.iter().any(|(id, _)| id == m),
                    "unknown menu {m} for {}",
                    c.id
                );
            }
            if let Some(rb) = &c.ribbon {
                assert!(
                    RIBBON_GROUPS
                        .iter()
                        .any(|(t, g, _)| *t == rb.tab && *g == rb.group),
                    "unknown ribbon group {}/{} for {}",
                    rb.tab,
                    rb.group,
                    c.id
                );
            }
            if let Some(p) = &c.pane {
                assert!(
                    PANES.iter().any(|(id, _)| id == p),
                    "unknown pane {p} for {}",
                    c.id
                );
            }
            assert!(
                c.menu.is_some()
                    || c.ribbon.is_some()
                    || c.pane.is_some()
                    || MENUS.iter().any(|(m, _)| *m == c.id),
                "unplaced {}",
                c.id
            );
        }
    }

    #[test]
    fn every_menu_and_tab_has_content() {
        let r = registry();
        for (menu, _) in MENUS {
            assert!(!r.menu_entries(menu).is_empty(), "menu {menu} is empty");
        }
        for (tab, _) in RIBBON_TABS {
            let groups = r.ribbon_groups(tab);
            assert!(!groups.is_empty(), "tab {tab} has no groups");
            for (g, _) in groups {
                assert!(!r.ribbon_group(tab, g).is_empty(), "group {tab}/{g} is empty");
            }
        }
    }

    #[test]
    fn menu_order_hints_reference_known_commands() {
        let r = registry();
        for (_, order) in MENU_ORDER {
            for id in order.iter().filter(|i| **i != "-") {
                assert!(r.get(id).is_some(), "menu order references unknown command {id}");
            }
        }
    }

    #[test]
    fn search_finds_by_label_and_path() {
        let r = registry();
        let hits = r.search("watermark", 10);
        assert!(
            hits.iter().any(|c| c.id.starts_with("document.watermarks")),
            "{:?}",
            hits.iter().map(|c| &c.id).collect::<Vec<_>>()
        );
        assert_eq!(
            r.search("Save As", 5).first().map(|c| c.id.as_str()),
            Some("saveAs")
        );
        assert!(r.path_label("document.watermarks.add").starts_with("Document › "));
    }

    #[test]
    fn shortcuts_are_unique() {
        let r = registry();
        let mut seen = HashMap::new();
        for c in r.all() {
            if let Some(s) = &c.shortcut {
                if let Some(prev) = seen.insert(s.to_lowercase(), c.id.clone()) {
                    panic!("shortcut {s} used by {} and {}", prev, c.id);
                }
            }
        }
    }
}
