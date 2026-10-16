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
        id: "home",
        label: "Home",
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
                "edit",
                "Edit",
                [
                    "undo",
                    "redo",
                    "copy",
                    "paste",
                    "delete",
                    "find",
                    "findAndRepl",
                    "selectAll",
                    "cut"
                ]
            ),
            group!(
                "objects",
                "Objects",
                [
                    "tool.addText",
                    "document.addImage",
                    "tool.selBarcodePlace",
                    "tool.editContent",
                    "tool.editText"
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
                "links",
                "Links",
                [
                    "addLink",
                    "tool.annot.editLink",
                    "createWebLinksFromText",
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
];

pub const CONTEXT_TAB: &str = "ctx.format";

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
const COL_GAP: f64 = 2.0;
/// Large buttons kept per group when the ribbon mixes large and small buttons.
const MAX_LARGE_MIXED: usize = 2;

/// What the fitting algorithm needs to know about a group.
#[derive(Debug, Clone)]
pub struct GroupSpec {
    /// Width of each curated item drawn as a small (icon + label) button.
    pub small_w: Vec<f64>,
    /// More registry commands exist, so an overflow button is always needed.
    pub extra: bool,
    /// Estimated width of the group title.
    pub title: f64,
    /// Width of the small "More" button.
    pub more_w: f64,
}

/// How a group is drawn: the first `large` items as large buttons, the next
/// `small` items as small buttons stacked three per column, the rest in an
/// overflow menu. `(0, 0)` collapses the whole group into one button.
pub type GroupFit = (usize, usize);

/// Estimated width of a small ribbon button with this label.
pub fn small_width(label: &str) -> f64 {
    30.0 + (label.chars().count() as f64 * 6.7).min(132.0)
}

fn group_width(spec: &GroupSpec, (large, small): GroupFit) -> f64 {
    if large == 0 && small == 0 {
        return PAD + LARGE_W.max(spec.title);
    }
    let n = spec.small_w.len();
    let overflow = large + small < n || spec.extra;
    let mut w = large as f64 * LARGE_W;
    let mut stack: Vec<f64> = spec.small_w[large..large + small].to_vec();
    if overflow {
        if small > 0 {
            stack.push(spec.more_w);
        } else {
            w += LARGE_W;
        }
    }
    for col in stack.chunks(3) {
        w += col.iter().fold(0.0f64, |a, b| a.max(*b)) + COL_GAP;
    }
    PAD + w.max(spec.title)
}

fn total_width(specs: &[GroupSpec], fit: &[GroupFit]) -> f64 {
    specs
        .iter()
        .zip(fit)
        .map(|(s, f)| group_width(s, *f))
        .sum::<f64>()
        + GAP * specs.len().saturating_sub(1) as f64
}

/// Choose a layout for every group so the ribbon fits `width` pixels.
///
/// All-large buttons are used when they fit. Otherwise each group shows its
/// items as small buttons (dropping the least important ones into the
/// overflow menu, round-robin from the largest group) and then promotes up to
/// two leading items per group to large buttons while space allows. Groups
/// collapse into a single button, from the last one, only as a last resort.
pub fn fit_ribbon(specs: &[GroupSpec], width: f64) -> Vec<GroupFit> {
    let all_large: Vec<GroupFit> = specs.iter().map(|s| (s.small_w.len(), 0)).collect();
    if total_width(specs, &all_large) <= width {
        return all_large;
    }
    let mut fit: Vec<GroupFit> = specs.iter().map(|s| (0, s.small_w.len())).collect();
    while total_width(specs, &fit) > width {
        let candidate = (0..fit.len())
            .filter(|i| fit[*i].0 + fit[*i].1 > 1)
            .max_by_key(|i| fit[*i].0 + fit[*i].1);
        match candidate {
            Some(i) => {
                if fit[i].1 > 0 {
                    fit[i].1 -= 1;
                } else {
                    fit[i].0 -= 1;
                }
            }
            None => match (0..fit.len()).rev().find(|i| fit[*i] != (0, 0)) {
                Some(i) => fit[i] = (0, 0),
                None => break,
            },
        }
    }
    // Promote leading items to large buttons, round-robin, while it fits.
    loop {
        let mut changed = false;
        for i in 0..fit.len() {
            let (l, s) = fit[i];
            if s == 0 || l >= MAX_LARGE_MIXED {
                continue;
            }
            let before = fit[i];
            fit[i] = (l + 1, s - 1);
            if total_width(specs, &fit) > width {
                fit[i] = before;
            } else {
                changed = true;
            }
        }
        if !changed {
            break;
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
            extra,
            title,
            more_w: small_width("More (12)"),
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
            let fit = fit_ribbon(&specs, width);
            let used = total_width(&specs, &fit);
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
            fit_ribbon(&specs, 10_000.0),
            vec![(6, 0), (7, 0), (9, 0), (5, 0), (11, 0)]
        );
        let narrow = fit_ribbon(&specs, 936.0);
        let visible: usize = narrow.iter().map(|(l, s)| l + s).sum();
        // The old one-large-button-per-group layout showed at most 5 commands here.
        assert!(visible >= 10, "{narrow:?}");
        assert!(
            narrow.iter().all(|f| *f != (0, 0)),
            "no group should collapse at 936 px: {narrow:?}"
        );
    }
}
