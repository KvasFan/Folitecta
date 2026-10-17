//! Curated ribbon layout: tabs, groups and the order of their buttons.
//!
//! The command registry places every command in a tab and group; this table
//! chooses which commands get a button and in which order (most important first).
//! Registry commands of the same group that are not listed here stay reachable
//! from the group's overflow menu.

pub struct RGroup {
    pub id: &'static str,
    pub label: &'static str,
    /// Registry ids, in priority order.
    pub items: &'static [&'static str],
}

pub struct RTab {
    pub id: &'static str,
    pub label: &'static str,
    pub groups: &'static [RGroup],
}

macro_rules! group {
    ($id:literal, $label:literal, [$($item:literal),* $(,)?]) => {
        RGroup { id: $id, label: $label, items: &[$($item),*] }
    };
}

pub const RIBBON: &[RTab] = &[
    RTab {
        // The first tab (id "home", shown as Edit): tools, then the editing
        // of the text, the clipboard, the objects and comments of the pages.
        id: "home",
        label: "Edit",
        groups: &[
            group!(
                "tools",
                "Tools",
                [
                    "tool.hand",
                    "tool.selectText",
                    "tool.snapshot",
                    "tool.zoom",
                    "tool.loupe",
                    "tool.panZoom"
                ]
            ),
            group!(
                "text",
                "Text",
                [
                    "tool.editText",
                    "pageText.insert",
                    "pageText.replace",
                    "pageText.delete",
                    "find",
                    "findAndRepl"
                ]
            ),
            group!(
                "edit",
                "Clipboard",
                ["undo", "redo", "cut", "copy", "paste", "delete", "selectAll"]
            ),
            group!(
                "objects",
                "Objects",
                [
                    "tool.addText",
                    "document.addImage",
                    "tool.selBarcodePlace",
                    "tool.editContent"
                ]
            ),
            group!(
                "comment",
                "Comment",
                [
                    "tool.annot.stickyNote",
                    "tool.annot.typeWriter",
                    "tool.annot.highlight",
                    "tool.annot.underline",
                    "tool.annot.strikeout",
                    "tool.annot.textBox",
                    "tool.annot.square",
                    "tool.annot.circle",
                    "tool.annot.arrow",
                    "tool.annot.pencil",
                    "tool.annot.stamp"
                ]
            ),
            group!(
                "view",
                "View",
                [
                    "zoom.level.fitWidth",
                    "zoom.level.fitPage",
                    "zoom.level.actual",
                    "zoomIn",
                    "zoomOut",
                    "view.rotate",
                    "view.layout"
                ]
            ),
            group!(
                "links",
                "Links",
                [
                    "addLink",
                    "tool.annot.editLink",
                    "createWebLinksFromText",
                    "createWebLinksFromBarcodes",
                    "removeAllWebLinks"
                ]
            ),
            group!(
                "protect",
                "Protect",
                [
                    "tool.annot.redaction",
                    "document.applyRedactions",
                    "document.security",
                    "document.signatures.sign",
                    "tool.sign.and.initials"
                ]
            ),
        ],
    },
    RTab {
        id: "view",
        label: "View",
        groups: &[
            group!(
                "tools",
                "Tools",
                ["tool.hand", "tool.selectText", "view.loupe", "view.panzoom"]
            ),
            group!(
                "goto",
                "Go To",
                [
                    "view.goto.firstPage",
                    "view.goto.prevPage",
                    "view.goto.nextPage",
                    "view.goto.lastPage",
                    "view.goto.page",
                    "view.goto.prevView",
                    "view.goto.nextView"
                ]
            ),
            group!(
                "zoom",
                "Zoom",
                [
                    "zoomIn",
                    "zoomOut",
                    "zoom.level.actual",
                    "zoom.level.fitPage",
                    "zoom.level.fitWidth",
                    "zoom.to.level",
                    "zoomSel"
                ]
            ),
            group!(
                "pageDisplay",
                "Page Display",
                [
                    "view.layout.onePage",
                    "view.layout.oneColumn",
                    "view.layout.twoPages",
                    "view.layout.twoColumns",
                    "view.rotate.CW",
                    "view.rotate.CCW",
                    "view.showRulers",
                    "view.showGrid",
                    "view.showGuides",
                    "view.darkPageMode",
                    "view.fullScreen"
                ]
            ),
            group!(
                "panes",
                "Panes",
                [
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
                    "view.properties"
                ]
            ),
            group!(
                "window",
                "Window",
                [
                    "window.new",
                    "window.split",
                    "window.removeSplit",
                    "view.theme",
                    "view.ribbonLabels",
                    "view.ribbonClassicUI"
                ]
            ),
        ],
    },
    RTab {
        id: "comment",
        label: "Comment",
        groups: &[
            group!(
                "tools",
                "Tools",
                ["tool.editAnnots", "tool.selectText", "tool.hand"]
            ),
            group!(
                "text",
                "Text",
                [
                    "tool.annot.typeWriter",
                    "tool.annot.textBox",
                    "tool.annot.callout"
                ]
            ),
            group!(
                "note",
                "Note",
                [
                    "tool.annot.stickyNote",
                    "tool.annot.fileAttachment",
                    "tool.annot.sound"
                ]
            ),
            group!(
                "textMarkup",
                "Text Markup",
                [
                    "tool.annot.highlight",
                    "tool.annot.underline",
                    "tool.annot.strikeout",
                    "tool.annot.insertText",
                    "tool.annot.replaceText"
                ]
            ),
            group!(
                "drawing",
                "Drawing",
                [
                    "tool.annot.square",
                    "tool.annot.circle",
                    "tool.annot.line",
                    "tool.annot.arrow",
                    "tool.annot.polygon",
                    "tool.annot.polyline",
                    "tool.annot.cloud",
                    "tool.annot.pencil",
                    "tool.annot.highlightArea",
                    "tool.annot.eraser",
                    "tool.annot.stamp"
                ]
            ),
            group!(
                "measure",
                "Measure",
                [
                    "tool.annot.distance",
                    "tool.annot.perimeter",
                    "tool.annot.area",
                    "tool.calibrate",
                    "exportMeasuresToCSV"
                ]
            ),
            group!(
                "manage",
                "Manage Comments",
                [
                    "view.comments",
                    "comments.summarize",
                    "comments.import",
                    "comments.export",
                    "comments.flatten",
                    "comments.deleteAllDoc",
                    "comments.showhideAll",
                    "comments.previous",
                    "comments.next"
                ]
            ),
        ],
    },
    RTab {
        id: "protect",
        label: "Protect",
        groups: &[
            group!(
                "digiSigns",
                "Digital Signatures",
                [
                    "document.signatures.sign",
                    "document.signatures.certify",
                    "document.signatures.validateAll",
                    "document.signatures.timestamp",
                    "signs.digitalIDs",
                    "document.signatures.clear"
                ]
            ),
            group!(
                "signsAndInitials",
                "Signatures and Initials",
                ["tool.sign.and.initials", "signs.manage", "tool.signMarkup"]
            ),
            group!(
                "redact",
                "Redact",
                [
                    "tool.annot.redaction",
                    "document.applyRedactions",
                    "annots.applyRedactions",
                    "addAnnotsBySearchResult.redaction"
                ]
            ),
            group!(
                "docSecurity",
                "Document Security",
                [
                    "document.security",
                    "standardSecPolicy",
                    "securityPolicies",
                    "saveUnrestricted"
                ]
            ),
        ],
    },
    RTab {
        id: "form",
        label: "Form",
        groups: &[
            group!(
                "fields",
                "Form Fields",
                [
                    "tool.editFields",
                    "tool.editFields.mode.text",
                    "tool.editFields.mode.check",
                    "tool.editFields.mode.radio",
                    "tool.editFields.mode.combo",
                    "tool.editFields.mode.list",
                    "tool.editFields.mode.button",
                    "tool.editFields.mode.signature",
                    "fieldsView.tabOrder"
                ]
            ),
            group!(
                "data",
                "Form Data",
                [
                    "forms.highlight",
                    "forms.reset",
                    "forms.importData",
                    "forms.exportData",
                    "forms.populate",
                    "forms.calcOrder"
                ]
            ),
            group!(
                "javaScript",
                "JavaScript",
                ["document.javaScript", "document.actions", "jsConsole"]
            ),
        ],
    },
    RTab {
        id: "organize",
        label: "Organize",
        groups: &[
            group!(
                "pages",
                "Pages",
                [
                    "document.insertPages",
                    "document.insertEmptyPages",
                    "document.deletePages",
                    "document.extractPages",
                    "document.insertImages",
                    "document.replacePages",
                    "document.duplicatePages",
                    "document.movePages",
                    "document.swapPages",
                    "document.reversePageOrder",
                    "document.split",
                    "document.splitPages",
                    "document.mergePages",
                    "document.deleteEmptyPages",
                    "document.findDuplicates"
                ]
            ),
            group!(
                "transformPages",
                "Transform Pages",
                [
                    "document.rotatePages",
                    "document.cropPages",
                    "document.cropToWhiteMargins",
                    "document.resizePages",
                    "document.setMargins",
                    "document.normalizePages",
                    "document.overlayPages",
                    "document.rasterizePages"
                ]
            ),
            group!(
                "pageMarks",
                "Page Marks",
                [
                    "document.watermarks",
                    "document.headerAndFooter",
                    "document.batesNumbering",
                    "document.background",
                    "document.pageNumbering",
                    "document.pageTransitions"
                ]
            ),
        ],
    },
    RTab {
        id: "convert",
        label: "Convert",
        groups: &[
            group!(
                "create",
                "Create",
                [
                    "newDocBlank",
                    "newDocCombine",
                    "newDocImage",
                    "newDocText",
                    "newDocClipboard",
                    "newDocEmail",
                    "newPortfolio"
                ]
            ),
            group!(
                "export",
                "Export",
                ["document.export.toImages", "export", "document.exportBookmarks"]
            ),
            group!(
                "modify",
                "Modify",
                [
                    "document.recompressImages",
                    "document.discardPDFA",
                    "document.replaceFonts",
                    "document.audit",
                    "document.wordCount",
                    "document.detectImageBarcode",
                    "document.detectBarcodes",
                    "xmp"
                ]
            ),
            group!(
                "ocr",
                "OCR",
                ["document.OCRPages", "document.OCRSelRgn", "document.ocrImages"]
            ),
            group!(
                "enhancePages",
                "Enhance Pages",
                ["imageEditor", "replaceImage", "document.rasterizePages"]
            ),
        ],
    },
    RTab {
        id: "share",
        label: "Share",
        groups: &[
            group!(
                "sendTo",
                "Send",
                ["sendByEmail", "sendByEmailZip", "sendByEmailOpenAll"]
            ),
            group!("places", "Places", ["openFromPlaces", "saveToPlaces", "explorer"]),
        ],
    },
    RTab {
        id: "review",
        label: "Review",
        groups: &[
            group!(
                "proofing",
                "Proofing",
                [
                    "view.spellCheck",
                    "document.spellCheckAndRepl",
                    "document.wordCount"
                ]
            ),
            group!(
                "comments",
                "Comments",
                [
                    "compareDocs",
                    "comments.summarize",
                    "comments.previous",
                    "comments.next"
                ]
            ),
        ],
    },
    RTab {
        id: "acc",
        label: "Accessibility",
        groups: &[
            group!(
                "autotag",
                "Auto-Tag",
                ["acc.autotag.document", "acc.autotag.form"]
            ),
            group!("check", "Check", ["acc.check.document", "acc.check.report"]),
            group!("edit", "Edit", ["view.tags", "view.order", "acc.alt"]),
        ],
    },
    RTab {
        id: "help",
        label: "Help",
        groups: &[
            group!(
                "help",
                "Help",
                ["help.contents", "help.cmdline", "help.support", "help.report"]
            ),
            group!(
                "product",
                "Product",
                ["help.about", "help.checkForUpdates", "help.homepage"]
            ),
            group!(
                "ui",
                "UI Options",
                [
                    "prefs",
                    "view.theme",
                    "view.ribbonLabels",
                    "settings.export",
                    "settings.import",
                    "settings.reset"
                ]
            ),
        ],
    },
    // Contextual tabs (shown while a drawing tool or a comment is active).
    RTab {
        id: "ctx.format",
        label: "Format",
        groups: &[
            group!(
                "shapeStyle",
                "Shape Style",
                [
                    "scolor",
                    "fcolor",
                    "opacity",
                    "borderWidth",
                    "borderStyle",
                    "lineStart",
                    "lineEnd"
                ]
            ),
            group!(
                "textFormat",
                "Text Format",
                ["fontName", "fontSize", "textStyle", "textAlign"]
            ),
        ],
    },
    // Shown while comments are selected: aligning, distributing, sizing and
    // stacking them.
    RTab {
        id: "ctx.arrange",
        label: "Arrange",
        groups: &[
            group!(
                "align",
                "Align",
                [
                    "align.left",
                    "align.center",
                    "align.right",
                    "align.top",
                    "align.middle",
                    "align.bottom",
                    "alignto"
                ]
            ),
            group!(
                "distribute",
                "Distribute",
                ["distribHorz", "distribVert", "distributeTo"]
            ),
            group!(
                "size",
                "Size",
                ["makeSameWidth", "makeSameHeight", "makeSameSize"]
            ),
            group!(
                "zOrder",
                "Order",
                [
                    "order.foreground",
                    "order.moveForward",
                    "order.moveBackward",
                    "order.background"
                ]
            ),
        ],
    },
    // Shown while a calculator tape is being edited.
    RTab {
        id: "ctx.calculator",
        label: "Calculator",
        groups: &[group!(
            "tape",
            "Tape",
            [
                "ac.addLine",
                "ac.delLine",
                "ac.clearTape",
                "ac.undo",
                "ac.redo",
                "ac.settings"
            ]
        )],
    },
];

pub const CONTEXT_TAB: &str = "ctx.format";
/// The contextual tab of the selected comments.
pub const ARRANGE_TAB: &str = "ctx.arrange";

/// Find a tab by id.
pub fn tab(id: &str) -> Option<&'static RTab> {
    RIBBON.iter().find(|t| t.id == id)
}

/// Main (non-contextual) tabs.
pub fn main_tabs() -> impl Iterator<Item = &'static RTab> {
    RIBBON.iter().filter(|t| !t.id.starts_with("ctx."))
}

/// Width of a large ribbon button (62 px plus spacing).
pub const LARGE_W: f64 = 64.0;
const PAD: f64 = 14.0;
const GAP: f64 = 6.0;
/// Space between two columns of small buttons.
pub const COL_GAP: f64 = 2.0;
/// Width of a tool drawn as an icon alone (the tool names hidden).
pub const CELL_W: f64 = 26.0;
/// Large buttons kept per group when the ribbon mixes large and small buttons.
const MAX_LARGE_MIXED: usize = 2;

/// What the fitting algorithm needs to know about a group.
#[derive(Debug, Clone)]
pub struct GroupSpec {
    /// Width of each curated item drawn as a small (icon + label) button.
    pub small_w: Vec<f64>,
    /// Width of each curated item drawn as a large button (wide enough for its
    /// longest word, so labels never break inside a word).
    pub large_w: Vec<f64>,
    /// More registry commands exist, so an overflow button is always needed.
    pub extra: bool,
    /// Estimated width of the group title.
    pub title: f64,
    /// Width of the small "More" button.
    pub more_w: f64,
    /// Width of the single button that replaces a collapsed group.
    pub collapsed_w: f64,
    /// Tools shown as small buttons whatever the width of the window, when
    /// the user chose how wide the group is (by dragging its edge).
    pub fixed: Option<usize>,
}

/// Rows of small buttons in a column of the ribbon, when the user has not
/// changed the height of the ribbon.
pub const DEFAULT_ROWS: usize = 3;
/// Height of one row of small buttons, in pixels.
pub const ROW_H: f64 = 21.0;
/// Width of a column of small buttons, used to turn a drag into tools.
pub const COLUMN_W: f64 = 130.0;

/// Height of the ribbon body for `rows` rows of small buttons.
pub fn body_height(rows: usize) -> f64 {
    29.0 + rows as f64 * ROW_H
}

/// Icons in each column of a group of `n` icons: at most `rows`, fewer when
/// the title of the group is wider than the columns, so the icons spread
/// under the title instead of leaving it alone. The group stays as wide as
/// [`group_width`] says, since it is at least as wide as its title.
pub fn icons_per_column(n: usize, rows: usize, title: f64) -> usize {
    let rows = rows.max(1);
    if n == 0 {
        return rows;
    }
    let under_title = (title / (CELL_W + COL_GAP)).floor() as usize;
    let columns = n.div_ceil(rows).max(under_title.min(n)).max(1);
    n.div_ceil(columns)
}

/// How a group is drawn: the first `large` items as large buttons, the next
/// `small` items as small buttons stacked three per column, the rest in an
/// overflow menu. `(0, 0)` collapses the whole group into one button.
pub type GroupFit = (usize, usize);

/// Estimated width of a small ribbon button with this label.
pub fn small_width(label: &str) -> f64 {
    30.0 + (label.chars().count() as f64 * 6.7).min(132.0)
}

/// Estimated width of a large ribbon button with this label.
pub fn large_width(label: &str) -> f64 {
    let longest = label
        .split_whitespace()
        .map(|w| w.chars().count())
        .max()
        .unwrap_or(0) as f64;
    (longest * 6.4 + 14.0).clamp(LARGE_W, 104.0)
}

fn visible(f: GroupFit) -> usize {
    f.0 + f.1
}

pub fn group_width(spec: &GroupSpec, (large, small): GroupFit, rows: usize) -> f64 {
    if large == 0 && small == 0 {
        return PAD + spec.collapsed_w.max(spec.title);
    }
    let n = spec.small_w.len();
    let overflow = large + small < n || spec.extra;
    let mut w: f64 = spec.large_w[..large].iter().sum();
    let mut stack: Vec<f64> = spec.small_w[large..large + small].to_vec();
    if overflow {
        if small > 0 {
            stack.push(spec.more_w);
        } else {
            w += LARGE_W;
        }
    }
    for col in stack.chunks(rows.max(1)) {
        w += col.iter().fold(0.0f64, |a, b| a.max(*b)) + COL_GAP;
    }
    PAD + w.max(spec.title)
}

fn total_width(specs: &[GroupSpec], fit: &[GroupFit], rows: usize) -> f64 {
    specs
        .iter()
        .zip(fit)
        .map(|(s, f)| group_width(s, *f, rows))
        .sum::<f64>()
        + GAP * specs.len().saturating_sub(1) as f64
}

/// Drop items from the largest groups (later groups first on ties) while the
/// ribbon is too wide and `allowed` permits it. Groups of a fixed width keep
/// their tools.
fn shrink(
    specs: &[GroupSpec],
    fit: &mut [GroupFit],
    width: f64,
    rows: usize,
    allowed: impl Fn(usize, GroupFit) -> bool,
) {
    while total_width(specs, fit, rows) > width {
        let candidate = (0..fit.len())
            .filter(|i| fit[*i] != (0, 0) && specs[*i].fixed.is_none() && allowed(*i, fit[*i]))
            .max_by_key(|i| visible(fit[*i]));
        match candidate {
            Some(i) if fit[i].1 > 0 => fit[i].1 -= 1,
            Some(i) => fit[i].0 -= 1,
            None => break,
        }
    }
}

/// Choose a layout for every group so the ribbon fits `width` pixels.
///
/// All-large buttons are used when they fit. Otherwise every group shows its
/// commands as small buttons, dropping the least important ones into its
/// overflow menu while keeping at least two per group; if that is still too
/// wide, groups collapse into a single button starting from the right, and
/// only then shrink to one command. Any space left is given back (collapsed
/// groups first, left to right) and up to two leading commands per group
/// become large buttons.
///
/// Small buttons stack `rows` per column; large buttons need three rows, and
/// a width for each tool in `large_w` (none when the tools are icons alone).
/// Groups of a fixed width show their chosen number of tools as small
/// buttons and take no part in the fitting: when they make the ribbon wider
/// than the window, the ribbon scrolls.
pub fn fit_ribbon(specs: &[GroupSpec], width: f64, rows: usize) -> Vec<GroupFit> {
    let rows = rows.max(1);
    let large_ok = rows >= DEFAULT_ROWS && specs.iter().all(|s| s.large_w.len() == s.small_w.len());
    let n = specs.len();
    let fixed = |j: usize| specs[j].fixed.map(|k| (0, k.min(specs[j].small_w.len()).max(1)));
    let free = |j: usize| specs[j].fixed.is_none();
    let all_large: Vec<GroupFit> = (0..n)
        .map(|j| fixed(j).unwrap_or((specs[j].small_w.len(), 0)))
        .collect();
    if large_ok && total_width(specs, &all_large, rows) <= width {
        return all_large;
    }
    let mut fit: Vec<GroupFit> = (0..n)
        .map(|j| fixed(j).unwrap_or((0, specs[j].small_w.len())))
        .collect();
    shrink(specs, &mut fit, width, rows, |i, f| {
        visible(f) > specs[i].small_w.len().min(2)
    });
    let mut i = n;
    while total_width(specs, &fit, rows) > width && i > 1 {
        i -= 1;
        if free(i) {
            fit[i] = (0, 0);
        }
    }
    shrink(specs, &mut fit, width, rows, |_, f| visible(f) > 1);
    if total_width(specs, &fit, rows) > width && n > 0 && free(0) {
        fit[0] = (0, 0);
    }
    // Give room back: re-open collapsed groups and re-add commands, left to right.
    loop {
        let mut changed = false;
        for j in (0..n).filter(|j| free(*j)) {
            let (l, s) = fit[j];
            if l + s >= specs[j].small_w.len() {
                continue;
            }
            fit[j] = (l, s + 1);
            if total_width(specs, &fit, rows) > width {
                fit[j] = (l, s);
            } else {
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // Promote leading commands to large buttons, round-robin, while it fits.
    let mut changed = large_ok;
    while changed {
        changed = false;
        for j in (0..n).filter(|j| free(*j)) {
            let (l, s) = fit[j];
            if s == 0 || l >= MAX_LARGE_MIXED {
                continue;
            }
            fit[j] = (l + 1, s - 1);
            if total_width(specs, &fit, rows) > width {
                fit[j] = (l, s);
            } else {
                changed = true;
            }
        }
    }
    fit
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curated_items_exist_in_registry() {
        let reg = folitecta_core::registry();
        for t in RIBBON {
            for g in t.groups {
                for id in g.items {
                    assert!(
                        reg.get(id).is_some(),
                        "ribbon {}/{} lists unknown command {id}",
                        t.id,
                        g.id
                    );
                }
            }
        }
    }

    fn specs() -> Vec<GroupSpec> {
        let mk = |n: usize, extra: bool, title: f64| GroupSpec {
            small_w: (0..n).map(|i| 60.0 + (i % 4) as f64 * 15.0).collect(),
            large_w: vec![LARGE_W; n],
            collapsed_w: LARGE_W,
            extra,
            title,
            more_w: small_width("More (12)"),
            fixed: None,
        };
        vec![
            mk(6, true, 40.0),
            mk(7, false, 40.0),
            mk(9, true, 40.0),
            mk(5, false, 50.0),
            mk(11, false, 70.0),
        ]
    }

    #[test]
    fn ribbon_fits_every_width() {
        let specs = specs();
        for width in [400.0, 700.0, 936.0, 1256.0, 1576.0, 1896.0] {
            let fit = fit_ribbon(&specs, width, DEFAULT_ROWS);
            let used = total_width(&specs, &fit, DEFAULT_ROWS);
            assert!(
                used <= width || fit.iter().all(|f| *f == (0, 0)),
                "width {width}: {fit:?} uses {used}"
            );
        }
    }

    #[test]
    fn wide_ribbon_uses_large_buttons_and_narrow_one_keeps_commands() {
        let specs = specs();
        assert_eq!(
            fit_ribbon(&specs, 10_000.0, DEFAULT_ROWS),
            vec![(6, 0), (7, 0), (9, 0), (5, 0), (11, 0)]
        );
        let narrow = fit_ribbon(&specs, 936.0, DEFAULT_ROWS);
        let visible: usize = narrow.iter().map(|(l, s)| l + s).sum();
        // The old one-large-button-per-group layout showed at most 5 commands here.
        assert!(visible >= 10, "{narrow:?}");
        assert!(
            narrow.iter().all(|f| *f != (0, 0)),
            "no group should collapse at 936 px: {narrow:?}"
        );
    }

    #[test]
    fn more_rows_show_more_tools_and_two_rows_have_no_large_buttons() {
        let specs = specs();
        let shown = |rows| -> usize { fit_ribbon(&specs, 936.0, rows).iter().map(|(l, s)| l + s).sum() };
        assert!(shown(5) > shown(3), "{} vs {}", shown(5), shown(3));
        assert!(fit_ribbon(&specs, 10_000.0, 2).iter().all(|(l, _)| *l == 0));
    }

    #[test]
    fn icons_alone_show_more_tools_and_no_large_buttons() {
        let labelled = specs();
        let icons: Vec<GroupSpec> = labelled
            .iter()
            .map(|s| GroupSpec {
                small_w: vec![CELL_W; s.small_w.len()],
                large_w: Vec::new(),
                more_w: CELL_W,
                collapsed_w: CELL_W,
                ..s.clone()
            })
            .collect();
        let shown = |specs: &[GroupSpec], width| -> usize {
            fit_ribbon(specs, width, DEFAULT_ROWS)
                .iter()
                .map(|(l, s)| l + s)
                .sum()
        };
        for width in [400.0, 700.0, 936.0] {
            assert!(shown(&icons, width) > shown(&labelled, width), "width {width}");
            let fit = fit_ribbon(&icons, width, DEFAULT_ROWS);
            assert!(fit.iter().all(|(l, _)| *l == 0), "{fit:?}");
            assert!(total_width(&icons, &fit, DEFAULT_ROWS) <= width, "{fit:?}");
        }
        // Every tool of these groups fits in a window of 936 px.
        assert_eq!(shown(&icons, 936.0), 38);
    }

    #[test]
    fn icons_spread_under_a_wide_title() {
        // Three rows, a narrow title: full columns.
        assert_eq!(icons_per_column(7, 3, 30.0), 3);
        // A title as wide as six icons: three icons in one row.
        assert_eq!(icons_per_column(3, 3, 6.0 * (CELL_W + COL_GAP)), 1);
        // Nine icons under a title of four: three columns of three, then four of three.
        assert_eq!(icons_per_column(9, 3, 4.0 * (CELL_W + COL_GAP)), 3);
        assert_eq!(icons_per_column(12, 3, 4.0 * (CELL_W + COL_GAP)), 3);
        assert_eq!(icons_per_column(8, 3, 4.0 * (CELL_W + COL_GAP)), 2);
        assert_eq!(icons_per_column(0, 3, 100.0), 3);
    }

    #[test]
    fn groups_of_a_fixed_width_keep_their_tools() {
        let mut specs = specs();
        specs[4].fixed = Some(11);
        let fit = fit_ribbon(&specs, 700.0, DEFAULT_ROWS);
        assert_eq!(fit[4], (0, 11));
        // The other groups still fit what is left, or collapse.
        assert!(fit[..4].iter().all(|f| visible(*f) <= 9));
        specs[4].fixed = Some(99);
        assert_eq!(fit_ribbon(&specs, 700.0, DEFAULT_ROWS)[4], (0, 11));
    }
}
