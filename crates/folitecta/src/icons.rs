//! A small original icon set (24x24 stroke paths) chosen by command id keywords.

use dioxus::prelude::*;

/// Returns the SVG path data for a named icon.
pub fn path(name: &str) -> &'static str {
    match name {
        "open" => "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
        "save" => "M5 3h11l3 3v15H5z M8 3v6h8V3 M8 21v-7h8v7",
        "new" => "M7 3h7l5 5v13H7z M14 3v5h5 M10.5 13h5 M13 10.5v5",
        "print" => "M7 8V3h10v5 M7 17H4v-7h16v7h-3 M7 14h10v7H7z",
        "close" => "M6 6l12 12 M18 6L6 18",
        "undo" => "M9 14 4 9l5-5 M4 9h10a6 6 0 0 1 0 12h-3",
        "redo" => "m15 14 5-5-5-5 M20 9H10a6 6 0 0 0 0 12h3",
        "zoom-in" => "M10.5 18a7.5 7.5 0 1 0 0-15 7.5 7.5 0 0 0 0 15z M21 21l-5-5 M10.5 7.5v6 M7.5 10.5h6",
        "zoom-out" => "M10.5 18a7.5 7.5 0 1 0 0-15 7.5 7.5 0 0 0 0 15z M21 21l-5-5 M7.5 10.5h6",
        "zoom" => "M10.5 18a7.5 7.5 0 1 0 0-15 7.5 7.5 0 0 0 0 15z M21 21l-5-5",
        "fit" => "M4 9V4h5 M20 9V4h-5 M4 15v5h5 M20 15v5h-5 M8 8h8v8H8z",
        "hand" => {
            "M8 13V5a1.5 1.5 0 0 1 3 0v6 M11 11V4a1.5 1.5 0 0 1 3 0v7 M14 11V5.5a1.5 1.5 0 0 1 3 0V13 M8 13l-2.2-2.2a1.6 1.6 0 0 0-2.3 2.3L8 18a6 6 0 0 0 5 3h1a6 6 0 0 0 6-6v-2"
        }
        "select" => "M4 4h4 M4 4v4 M20 4h-4 M20 4v4 M4 20h4 M4 20v-4 M20 20h-4 M20 20v-4 M9 9h6v6H9z",
        "rotate" => "M20 11a8 8 0 1 0-2.3 5.7 M20 4v7h-7",
        "rotate-ccw" => "M4 11a8 8 0 1 1 2.3 5.7 M4 4v7h7",
        "page" => "M6 3h8l4 4v14H6z M14 3v4h4",
        "pages" => "M8 3h7l4 4v11H8z M15 3v4h4 M5 7v14h11",
        "delete" => "M4 7h16 M10 11v6 M14 11v6 M6 7l1 13h10l1-13 M9 7V4h6v3",
        "insert" => "M12 5v14 M5 12h14",
        "extract" => "M12 3v12 M8 11l4 4 4-4 M4 17v3h16v-3",
        "crop" => "M6 2v14a2 2 0 0 0 2 2h14 M18 22V8a2 2 0 0 0-2-2H2",
        "resize" => "M4 4h7 M4 4v7 M20 20h-7 M20 20v-7 M4 4l16 16",
        "margins" => "M3 3h18v18H3z M7 7h10v10H7z",
        "watermark" => "M4 20 20 4 M6 9c0-2 1-3 3-3s3 1 3 3-1 3-3 3 M15 21c0-2 1-3 3-3s3 1 3 3",
        "header" => "M4 4h16 M4 8h16 M6 13h12v7H6z",
        "number" => "M5 9h14 M5 15h14 M9 4 7 20 M17 4l-2 16",
        "comment" => "M21 12a8 8 0 0 1-8 8H5l-2 2V12a8 8 0 0 1 8-8h2a8 8 0 0 1 8 8z",
        "note" => "M5 4h14v11l-4 4H5z M15 19v-4h4 M8 9h8 M8 12h5",
        "highlight" => "m9 11-6 6v3h9l3-3 M22 12 12 2l-4 4 10 10z",
        "rect" => "M4 5h16v14H4z",
        "ellipse" => "M12 20c5 0 9-3.6 9-8s-4-8-9-8-9 3.6-9 8 4 8 9 8z",
        "line" => "M4 20 20 4",
        "arrow" => "M4 20 20 4 M12 4h8v8",
        "pencil" => "M12 20h9 M16.5 3.5a2.1 2.1 0 1 1 3 3L7 19l-4 1 1-4z",
        "text" => "M4 6h16 M12 6v14 M8 20h8",
        "link" => "M10 13a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1 1 M14 11a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1-1",
        "bookmark" => "M6 3h12v18l-6-4-6 4z",
        "attachment" => "m21 11-9 9a6 6 0 0 1-8.5-8.5l9-9a4 4 0 0 1 5.7 5.7l-9 9a2 2 0 0 1-2.9-2.9l8.5-8.5",
        "layers" => "m12 2 9 5-9 5-9-5z M3 12l9 5 9-5 M3 17l9 5 9-5",
        "eye" => "M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
        "eye-off" => {
            "M3 3l18 18 M10.6 5.1A9.7 9.7 0 0 1 12 5c6.4 0 10 7 10 7a17 17 0 0 1-3.2 4.2 M6.6 6.6C3.9 8.4 2 12 2 12s3.6 7 10 7a9.6 9.6 0 0 0 5.4-1.6 M9.9 9.9a3 3 0 0 0 4.2 4.2"
        }
        "flatten" => "M3 17l9 5 9-5 M12 3v10 M8 9l4 4 4-4",
        "field" => "M3 6h18v12H3z M7 10v4 M11 10h6",
        "search" => "M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16z M21 21l-4.3-4.3",
        "signature" => "M3 17c3-6 5-6 6-2s2 4 4-2 3-2 8 1 M3 21h18",
        "lock" => "M6 11h12v10H6z M8 11V7a4 4 0 0 1 8 0v4",
        "shield" => "M12 3 4 6v6c0 5 3.5 8 8 9 4.5-1 8-4 8-9V6z",
        "form" => "M4 4h16v16H4z M8 9h8 M8 13h8 M8 17h5",
        "image" => "M4 5h16v14H4z M8 11a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3z M4 17l5-5 4 4 3-3 4 4",
        "export" => "M12 3v12 M8 7l4-4 4 4 M4 15v4a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-4",
        "import" => "M12 15V3 M8 11l4 4 4-4 M4 15v4a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-4",
        "mail" => "M3 6h18v12H3z M3 6l9 7 9-7",
        "share" => {
            "M18 8a3 3 0 1 0 0-6 3 3 0 0 0 0 6z M6 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z M18 22a3 3 0 1 0 0-6 3 3 0 0 0 0 6z M8.6 13.5l6.8 4 M15.4 6.5l-6.8 4"
        }
        "ocr" => "M4 8V5h3 M20 8V5h-3 M4 16v3h3 M20 16v3h-3 M8 12h8 M12 9v6",
        "spell" => "M4 15 7 6l3 9 M5 12h4 M14 13l2 2 4-4",
        "list" => "M8 6h12 M8 12h12 M8 18h12 M4 6h.01 M4 12h.01 M4 18h.01",
        "grid" => "M4 4h7v7H4z M13 4h7v7h-7z M4 13h7v7H4z M13 13h7v7h-7z",
        "sun" => {
            "M12 17a5 5 0 1 0 0-10 5 5 0 0 0 0 10z M12 2v2 M12 20v2 M4.9 4.9l1.4 1.4 M17.7 17.7l1.4 1.4 M2 12h2 M20 12h2 M4.9 19.1l1.4-1.4 M17.7 6.3l1.4-1.4"
        }
        "moon" => "M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z",
        "settings" => {
            "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"
        }
        "help" => "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20z M9.1 9a3 3 0 0 1 5.8 1c0 2-3 3-3 3 M12 17h.01",
        "info" => "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20z M12 16v-4 M12 8h.01",
        "first" => "M18 6 12 12l6 6 M6 6v12",
        "last" => "m6 6 6 6-6 6 M18 6v12",
        "prev" => "m15 6-6 6 6 6",
        "next" => "m9 6 6 6-6 6",
        "up" => "m6 15 6-6 6 6",
        "down" => "m6 9 6 6 6-6",
        "fullscreen" => "M4 9V4h5 M20 9V4h-5 M4 15v5h5 M20 15v5h-5",
        "window" => "M3 5h18v14H3z M3 9h18",
        "single" => "M7 3h10v18H7z",
        "continuous" => "M7 2h10v9H7z M7 13h10v9H7z",
        "two" => "M3 4h8v16H3z M13 4h8v16h-8z",
        "clipboard" => "M9 4h6v3H9z M7 6H5v15h14V6h-2",
        "check" => "m5 12 5 5L20 7",
        "more" => "M5 12h.01 M12 12h.01 M19 12h.01",
        "star" => "m12 3 2.8 5.7 6.2.9-4.5 4.4 1.1 6.2L12 17.3 6.4 20.2l1.1-6.2L3 9.6l6.2-.9z",
        "calc" => {
            "M6 3h12v18H6z M9 7h6 M9 11h.01 M12 11h.01 M15 11h.01 M9 15h.01 M12 15h.01 M15 15h.01 M9 18h6"
        }
        "folder" => "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
        "redact" => "M4 6h16v12H4z M7 10h10 M7 14h6",
        "media" => "M4 5h16v14H4z M10 9v6l5-3z",
        "3d" => "m12 2 8 4.5v9L12 20l-8-4.5v-9z M12 11l8-4.5 M12 11v9 M12 11 4 6.5",
        "accessibility" => "M12 6a2 2 0 1 0 0-4 2 2 0 0 0 0 4z M5 9h14 M12 9v5 M12 14l-3 7 M12 14l3 7",
        "tag" => "M3 3h8l10 10-8 8L3 11z M7.5 7.5h.01",
        "tags" => "M8 6l-6 6 6 6 M16 6l6 6-6 6 M14 4l-4 16",
        "order" => {
            "M10 6h11 M10 12h11 M10 18h11 M4 4v4 M3 18h3l-3 3h3 M3 11.5a1.5 1.5 0 0 1 3 0c0 1-3 2-3 3h3"
        }
        "content" => "M4 4h16v6H4z M4 14h7v6H4z M15 14h5 M15 17h5 M15 20h5",
        "history" => "M3 12a9 9 0 1 0 3-6.7 M3 4v5h5 M12 7v5l3 3",
        "convert" => "M4 7h12l-3-3 M20 17H8l3 3",
        "split" => "M12 3v18 M5 8h4 M15 8h4 M5 16h4 M15 16h4",
        "merge" => "M8 3v6a4 4 0 0 0 4 4h0a4 4 0 0 0 4-4V3 M12 13v8 M8 17l4 4 4-4",
        "swap" => "M7 16V4m0 0L3 8m4-4 4 4 M17 8v12m0 0 4-4m-4 4-4-4",
        "move" => "M5 9l-3 3 3 3 M9 5l3-3 3 3 M15 19l-3 3-3-3 M19 9l3 3-3 3 M2 12h20 M12 2v20",
        "copy" => "M9 9h11v11H9z M5 15H4V4h11v1",
        "palette" => {
            "M12 2a10 10 0 0 0 0 20c1.4 0 2-1 2-2v-1a2 2 0 0 1 2-2h2a4 4 0 0 0 4-4 10 10 0 0 0-10-11z M7.5 11h.01 M10.5 7h.01 M14.5 7h.01"
        }
        _ => "M4 5h16v14H4z M8 9h8 M8 13h5",
    }
}

/// Pick an icon name for a command id.
pub fn for_command(id: &str) -> &'static str {
    let l = id.to_lowercase();
    let has = |s: &str| l.contains(s);
    // Specific tools first.
    match id {
        "tool.annot.distance" => return "line",
        "tool.annot.perimeter" => return "pencil",
        "tool.annot.area" => return "rect",
        "tool.calibrate" => return "settings",
        "tool.editFields.mode.text" => return "text",
        "tool.editFields.mode.check" => return "check",
        "tool.editFields.mode.radio" => return "select",
        "tool.editFields.mode.combo" | "tool.editFields.mode.list" => return "list",
        "tool.editFields.mode.button" => return "rect",
        "tool.editFields.mode.signature" => return "signature",
        "view.panzoom" | "tool.panZoom" => return "move",
        "launchExternalApp" => return "share",
        "view.layout.onePage" => return "single",
        "view.layout.oneColumn" => return "continuous",
        "view.layout.twoPages" | "view.layout.twoColumns" => return "two",
        _ => {}
    }
    if has("zoomin") || has("zoom.level.actual") {
        return "zoom-in";
    }
    if has("zoomout") {
        return "zoom-out";
    }
    if has("fit") && has("zoom") {
        return "fit";
    }
    if has("zoom") {
        return "zoom";
    }
    if has("undo") {
        return "undo";
    }
    if has("redo") {
        return "redo";
    }
    if has("saveas") || has("save") {
        return "save";
    }
    if has("open") || has("browse") {
        return "open";
    }
    if has("newdoc") || has("new") {
        return "new";
    }
    if has("print") {
        return "print";
    }
    if has("close") || has("exit") {
        return "close";
    }
    if has("rotate") && (has("ccw") || has("180")) {
        return "rotate-ccw";
    }
    if has("rotate") {
        return "rotate";
    }
    if has("crop") {
        return "crop";
    }
    if has("resize") || has("transform") {
        return "resize";
    }
    if has("margin") {
        return "margins";
    }
    if has("watermark") || has("background") {
        return "watermark";
    }
    if has("headerandfooter") || has("header") {
        return "header";
    }
    if has("bates") || has("numbering") || has("number") {
        return "number";
    }
    if has("delete") || has("remove") || has("clear") {
        return "delete";
    }
    if has("insert") || has("add") && has("page") {
        return "insert";
    }
    if has("extract") {
        return "extract";
    }
    if has("duplicate") || has("copy") {
        return "copy";
    }
    if has("split") {
        return "split";
    }
    if has("merge") || has("combine") {
        return "merge";
    }
    if has("swap") {
        return "swap";
    }
    if has("move") || has("order") || has("arrange") || has("align") || has("distrib") {
        return "move";
    }
    if has("stickynote") || has("note") {
        return "note";
    }
    if has("highlight") {
        return "highlight";
    }
    if has("square") || has("rect") {
        return "rect";
    }
    if has("circle") || has("ellipse") || has("oval") {
        return "ellipse";
    }
    if has("arrow") {
        return "arrow";
    }
    if has("line") && !has("underline") && !has("outline") {
        return "line";
    }
    if has("pencil") || has("ink") || has("freehand") {
        return "pencil";
    }
    if has("text") || has("font") || has("typewriter") {
        return "text";
    }
    if has("link") || has("uri") || has("url") {
        return "link";
    }
    if has("bookmark") {
        return "bookmark";
    }
    if has("attach") {
        return "attachment";
    }
    if has("layer") {
        return "layers";
    }
    if has("field") || has("form") {
        return "field";
    }
    if has("search") || has("find") {
        return "search";
    }
    if has("sign") {
        return "signature";
    }
    if has("secur") || has("encrypt") || has("password") || has("polic") {
        return "lock";
    }
    if has("redact") || has("sanitiz") {
        return "redact";
    }
    if has("protect") || has("cert") || has("pubsec") {
        return "shield";
    }
    if has("image") || has("picture") || has("scan") {
        return "image";
    }
    if has("export") {
        return "export";
    }
    if has("import") {
        return "import";
    }
    if has("email") || has("mail") {
        return "mail";
    }
    if has("share")
        || has("send")
        || has("cloud")
        || has("drive")
        || has("dropbox")
        || has("onedrive")
        || has("sharepoint")
        || has("box")
    {
        return "share";
    }
    if has("ocr") {
        return "ocr";
    }
    if has("spell") || has("proof") {
        return "spell";
    }
    if has("comment") || has("annot") || has("popup") || has("reply") || has("status") {
        return "comment";
    }
    if has("theme") || has("dark") {
        return "moon";
    }
    if has("prefs") || has("settings") || has("options") || has("customize") || has("ui") {
        return "settings";
    }
    if has("about") || has("info") || has("props") || has("properties") {
        return "info";
    }
    if has("help") || has("support") || has("report") {
        return "help";
    }
    if has("first") {
        return "first";
    }
    if has("last") {
        return "last";
    }
    if has("prev") || has("back") {
        return "prev";
    }
    if has("next") || has("forward") {
        return "next";
    }
    if has("fullscreen") {
        return "fullscreen";
    }
    if has("window") || has("tab") {
        return "window";
    }
    if has("onepage") || has("single") {
        return "single";
    }
    if has("column") || has("continuous") {
        return "continuous";
    }
    if has("twopages") {
        return "two";
    }
    if has("paste") || has("clipboard") || has("cut") {
        return "clipboard";
    }
    if has("hand") {
        return "hand";
    }
    if has("select") {
        return "select";
    }
    if has("calc") {
        return "calc";
    }
    if has("media") || has("sound") || has("video") || has("play") {
        return "media";
    }
    if has("3d") {
        return "3d";
    }
    if has("acc") || has("tag") {
        return "accessibility";
    }
    if has("history") || has("session") || has("recent") {
        return "history";
    }
    if has("convert") || has("pdfa") || has("optimiz") || has("recompress") {
        return "convert";
    }
    if has("thumbnail") || has("page") {
        return "pages";
    }
    if has("view") || has("show") || has("display") {
        return "grid";
    }
    if has("list") {
        return "list";
    }
    "page"
}

#[component]
pub fn Icon(name: &'static str, #[props(default = String::new())] class: String) -> Element {
    let d = path(name);
    rsx! {
        svg { class: "ic {class}", view_box: "0 0 24 24", "aria-hidden": "true", path { d: "{d}" } }
    }
}
