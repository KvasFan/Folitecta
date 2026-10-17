//! Toolbars of the classic (non-ribbon) interface. Each toolbar can be shown or
//! hidden from View > Toolbar; they wrap onto more rows when the window is narrow.

use dioxus::prelude::*;
use folitecta_core::registry;

use crate::state::AppState;
use crate::ui::chrome::ribbon::{BtnStyle, ribbon_button};

/// Toolbars: id (as in `view.toolbar.<id>`) and commands.
pub const TOOLBARS: &[(&str, &[&str])] = &[
    (
        "file",
        &["newDocBlank", "open", "save", "saveAs", "print", "close"],
    ),
    (
        "standard",
        &["undo", "redo", "cut", "copy", "paste", "delete", "find"],
    ),
    (
        "navigation",
        &[
            "view.goto.firstPage",
            "view.goto.prevPage",
            "view.goto.nextPage",
            "view.goto.lastPage",
            "view.goto.prevView",
            "view.goto.nextView",
        ],
    ),
    (
        "zoom",
        &[
            "zoomOut",
            "zoomIn",
            "zoom.level.actual",
            "zoom.level.fitPage",
            "zoom.level.fitWidth",
            "zoom.level.fitVisible",
        ],
    ),
    ("pageZoom", &["tool.zoom", "tool.loupe", "view.panzoom"]),
    (
        "pageLayout",
        &[
            "view.layout.onePage",
            "view.layout.oneColumn",
            "view.layout.twoPages",
            "view.layout.twoColumns",
        ],
    ),
    ("rotateview", &["view.rotate.CCW", "view.rotate.CW"]),
    ("properties", &["props"]),
    (
        "drawing",
        &[
            "tool.annot.stickyNote",
            "tool.annot.textBox",
            "tool.annot.typeWriter",
            "tool.annot.highlight",
            "tool.annot.square",
            "tool.annot.circle",
            "tool.annot.line",
            "tool.annot.arrow",
            "tool.annot.pencil",
            "tool.annot.stamp",
        ],
    ),
    (
        "measuring",
        &[
            "tool.annot.distance",
            "tool.annot.perimeter",
            "tool.annot.area",
            "tool.calibrate",
        ],
    ),
    (
        "form",
        &[
            "tool.editFields",
            "tool.editFields.mode.text",
            "tool.editFields.mode.check",
            "tool.editFields.mode.radio",
            "tool.editFields.mode.combo",
            "tool.editFields.mode.list",
            "tool.editFields.mode.button",
        ],
    ),
    (
        "contentEditing",
        &["tool.addText", "document.addImage", "tool.snapshot"],
    ),
    ("launchApp", &["launchExternalApp"]),
    ("docOptions", &["docProps", "document.security"]),
    (
        "view",
        &[
            "view.fullScreen",
            "view.showRulers",
            "view.showGrid",
            "view.showGuides",
        ],
    ),
];

/// Toolbars shown by default.
pub const DEFAULT_TOOLBARS: &[&str] = &["file", "standard", "navigation", "zoom", "drawing"];

/// Ids of the visible toolbars.
pub fn visible_toolbars(st: AppState) -> Vec<String> {
    match st.ui.read().value("toolbars") {
        Some(v) => v
            .split(',')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
        None => DEFAULT_TOOLBARS.iter().map(|s| s.to_string()).collect(),
    }
}

pub fn set_visible_toolbars(st: AppState, ids: &[String]) {
    // Keep the table order.
    let ordered: Vec<&str> = TOOLBARS
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| ids.iter().any(|v| v == id))
        .collect();
    st.set_value("toolbars", ordered.join(","));
}

/// Whether the menu bar shows its menus.
pub fn menubar_visible(st: AppState) -> bool {
    st.ui.read().value("menubar.hidden") != Some("1")
}

#[component]
pub fn ClassicToolbars() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let reg = registry();
    let visible = visible_toolbars(st);
    let bars: Vec<(&str, Vec<&'static folitecta_core::Command>)> = TOOLBARS
        .iter()
        .filter(|(id, _)| visible.iter().any(|v| v == id))
        .map(|(id, cmds)| (*id, cmds.iter().filter_map(|c| reg.get(c)).collect()))
        .collect();
    rsx! {
        div { class: "ribbon classic", onclick: move |e| e.stop_propagation(),
            div { class: "classic-toolbars",
                for (id, cmds) in bars {
                    div { class: "toolbar-group", key: "{id}",
                        for c in cmds {
                            {ribbon_button(st, lang, c, BtnStyle::Icon)}
                        }
                    }
                }
            }
        }
    }
}
