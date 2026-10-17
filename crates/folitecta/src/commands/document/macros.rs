//! Macros: placeholders of page-mark texts, inserted into the text field being
//! edited, and the external application launched with details of the view.

use dioxus::prelude::*;

use crate::commands::{handlers, no_doc};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind};

/// Placeholders of the external application's arguments: token and i18n key.
pub const LAUNCH_MACROS: &[(&str, &str)] = &[
    ("{path}", "macro.launch.path"),
    ("{currentpage}", "macro.launch.page"),
    ("{currentpage0}", "macro.launch.page0"),
    ("{zoom}", "macro.launch.zoom"),
    ("{rotation}", "macro.launch.rotation"),
    ("{x}", "macro.launch.x"),
    ("{y}", "macro.launch.y"),
    ("{xpx}", "macro.launch.xpx"),
    ("{ypx}", "macro.launch.ypx"),
];

/// Insert `token` into the text field focused last, selecting `select` inside
/// the inserted text when given. While a dialog is open the token goes into
/// one of its fields (the first text field when none of them was focused).
/// Without a field the token goes to the clipboard.
pub fn insert_token(st: AppState, token: &str, select: Option<&str>) {
    let tok = serde_json::to_string(token).unwrap_or_default();
    let sel = serde_json::to_string(&select.unwrap_or("")).unwrap_or_default();
    let js = format!(
        r#"
const tok = {tok}, sel = {sel};
let f = window.__foliField;
if (f && !document.contains(f)) f = null;
const dlg = document.querySelector('.dialog');
// A dialog is modal: its fields are the only ones being edited.
if (dlg && (!f || !dlg.contains(f))) f = dlg.querySelector('input[type=text], input:not([type]), textarea');
for (const s of document.querySelectorAll('.macro-picker select')) s.value = '';
if (!f) return false;
const s = f.selectionStart ?? f.value.length, e = f.selectionEnd ?? s;
f.focus();
f.setRangeText(tok, s, e, 'end');
if (sel) {{ const i = tok.indexOf(sel); if (i >= 0) f.setSelectionRange(s + i, s + i + sel.length); }}
f.dispatchEvent(new Event('input', {{ bubbles: true }}));
return true;
"#
    );
    let token = token.to_string();
    spawn(async move {
        let inserted = document::eval(&js)
            .await
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !inserted {
            crate::util::copy_to_clipboard(&token);
            st.toast(
                ToastKind::Info,
                tf(st.lang(), "toast.macro_copied", &[("macro", &token)]),
                "",
            );
        }
    });
}

fn put(st: AppState, token: &'static str) {
    insert_token(st, token, None);
}

/// Insert a macro with a part (a name or key) for the user to replace.
fn put_selecting(st: AppState, token: &'static str, part: &'static str) {
    insert_token(st, token, Some(part));
}

/// The Bates macro with the digits of the saved Bates format.
fn bates_token(st: AppState) -> String {
    let format = bates_format(st);
    let digits = format
        .find("{bates:")
        .and_then(|i| format[i + 7..].split('}').next())
        .and_then(|d| d.trim().parse::<usize>().ok())
        .unwrap_or(6);
    format!("{{bates:{digits}}}")
}

/// Bates numbering format saved by the Bates dialog.
pub fn bates_format(st: AppState) -> String {
    st.ui
        .peek()
        .value("bates.format")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("DOC-{bates:6}")
        .to_string()
}

/// Next Bates number (continues the previous numbering).
pub fn bates_next(st: AppState) -> i64 {
    st.ui
        .peek()
        .value("bates.next")
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(1)
}

/// Split a command line into arguments (double quotes group words).
pub fn split_args(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut has = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                has = true;
            }
            c if c.is_whitespace() && !quoted => {
                if has {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            c => {
                cur.push(c);
                has = true;
            }
        }
    }
    if has {
        out.push(cur);
    }
    out
}

/// Details of the view the launch macros refer to.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ViewInfo {
    pub path: String,
    pub page: usize,
    pub zoom: f32,
    pub rotation: i32,
    /// Top-left corner of the visible part of the page, in CSS pixels.
    pub x_px: f64,
    pub y_px: f64,
}

/// Replace the launch macros in an argument.
pub fn expand_launch(arg: &str, v: &ViewInfo) -> String {
    let z = v.zoom.max(0.01) as f64;
    arg.replace("{path}", &v.path)
        .replace("{currentpage0}", &v.page.to_string())
        .replace("{currentpage}", &(v.page + 1).to_string())
        .replace("{zoom}", &format!("{}", (v.zoom * 100.0).round() as i64))
        .replace("{rotation}", &v.rotation.to_string())
        .replace("{xpx}", &format!("{}", v.x_px.round() as i64))
        .replace("{ypx}", &format!("{}", v.y_px.round() as i64))
        .replace("{x}", &format!("{}", (v.x_px / z).round() as i64))
        .replace("{y}", &format!("{}", (v.y_px / z).round() as i64))
}

/// Run the external application with the saved program and arguments; without
/// a program the document opens in the system's PDF application.
pub fn launch(st: AppState, program: String, args: String) {
    let (path, dirty) = st
        .read_active(|t| (t.doc.path().map(|p| p.to_path_buf()), t.doc.is_dirty()))
        .unwrap_or((None, false));
    let Some(path) = path else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.save_first"), "");
        return;
    };
    // The application reads the file as saved.
    if dirty {
        st.toast(ToastKind::Warn, t(st.lang(), "toast.launch_unsaved"), "");
    }
    if program.trim().is_empty() {
        if let Err(e) = crate::util::open_external(&path.display().to_string()) {
            st.error(e);
        }
        return;
    }
    let zoom = crate::commands::current_zoom(st);
    let base = st
        .read_active(|t| ViewInfo {
            path: t.doc.path().map(|p| p.display().to_string()).unwrap_or_default(),
            page: t.page,
            zoom,
            rotation: t.view_rotation,
            ..Default::default()
        })
        .unwrap_or_default();
    let page = base.page;
    spawn(async move {
        let js = format!(
            "const c = document.querySelector('.canvas'), p = document.querySelector('#pg-{page} .page-inner'); \
             if (!c || !p) return [0, 0]; const cr = c.getBoundingClientRect(), pr = p.getBoundingClientRect(); \
             return [Math.max(0, cr.left - pr.left), Math.max(0, cr.top - pr.top)];"
        );
        let (x_px, y_px) = document::eval(&js)
            .await
            .ok()
            .and_then(|v| serde_json::from_value::<(f64, f64)>(v).ok())
            .unwrap_or((0.0, 0.0));
        let info = ViewInfo { x_px, y_px, ..base };
        let argv: Vec<String> = split_args(&args)
            .iter()
            .map(|a| expand_launch(a, &info))
            .collect();
        let program = program.trim().trim_matches('"').to_string();
        match std::process::Command::new(&program).args(&argv).spawn() {
            Ok(_) => st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.launched", &[("program", &program)]),
                "",
            ),
            Err(e) => st.error(format!("{program}: {e}")),
        }
    });
}

fn launch_saved(st: AppState) {
    if no_doc(st) {
        return;
    }
    st.set_dialog(Dialog::LaunchApp);
}

handlers! {
    "macro.guide" => |st| st.set_dialog(Dialog::MacroGuide),
    "macro.bates.options" => |st| { if !no_doc(st) { st.set_dialog(Dialog::Bates) } },
    "macro.page" => |st| put(st, "{page}"),
    "macro.pages" => |st| put(st, "{pages}"),
    "macro.pageLabel" => |st| put(st, "{label}"),
    "macro.srcPages" => |st| put(st, "{srcpages}"),
    "macro.imagenumber" => |st| put(st, "{imagenumber}"),
    "macro.bates" => |st| { let tok = bates_token(st); insert_token(st, &tok, None) },
    "macro.batesValue" => |st| { let tok = bates_format(st); insert_token(st, &tok, None) },
    "macro.autonumber" => |st| put(st, "{autonumber:4}"),
    "macro.docnumber" => |st| put(st, "{docnumber}"),
    "macro.date" => |st| put(st, "{date}"),
    "macro.time" => |st| put(st, "{time}"),
    "macro.year" => |st| put(st, "{year}"),
    "macro.month" => |st| put(st, "{month}"),
    "macro.day" => |st| put(st, "{day}"),
    "macro.hour" => |st| put(st, "{hour}"),
    "macro.minute" => |st| put(st, "{minute}"),
    "macro.seconds" => |st| put(st, "{second}"),
    "macro.docTitle" => |st| put(st, "{title}"),
    "macro.docInfo" => |st| put_selecting(st, "{docinfo:Subject}", "Subject"),
    "macro.fileName" => |st| put(st, "{file}"),
    "macro.fileExt" => |st| put(st, "{ext}"),
    "macro.folderName" => |st| put(st, "{foldername}"),
    "macro.folderTitle" => |st| put(st, "{foldername}"),
    "macro.folderPath" => |st| put(st, "{folder}"),
    "macro.bookmarkTitle" => |st| put(st, "{bookmark}"),
    "macro.computer" => |st| put(st, "{computer}"),
    "macro.user" => |st| put(st, "{user}"),
    "macro.env" => |st| put_selecting(st, "{env:NAME}", "NAME"),
    "macro.region" => |st| put(st, "{region}"),
    "macro.appVersion" => |st| put(st, "{app}"),
    "macro.LaunchApp.CurrentPage" => |st| put(st, "{currentpage}"),
    "macro.LaunchApp.CurrentPage0" => |st| put(st, "{currentpage0}"),
    "macro.LaunchApp.FileName" => |st| put(st, "{path}"),
    "macro.LaunchApp.PositionX" => |st| put(st, "{x}"),
    "macro.LaunchApp.PositionXpx" => |st| put(st, "{xpx}"),
    "macro.LaunchApp.PositionY" => |st| put(st, "{y}"),
    "macro.LaunchApp.PositionYpx" => |st| put(st, "{ypx}"),
    "macro.LaunchApp.ViewRotation" => |st| put(st, "{rotation}"),
    "macro.LaunchApp.Zoom" => |st| put(st, "{zoom}"),
    "launchExternalApp" => launch_saved,
    "quickLaunch" => |st| st.set_palette(Some(String::new())),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_split_and_expanded() {
        assert_eq!(
            split_args(r#"--page {currentpage} "C:\My Files\{path}" -z={zoom}"#),
            vec!["--page", "{currentpage}", r"C:\My Files\{path}", "-z={zoom}"]
        );
        assert_eq!(split_args(r#"a "" b"#), vec!["a", "", "b"]);
        let v = ViewInfo {
            path: "/tmp/a b.pdf".into(),
            page: 2,
            zoom: 1.5,
            rotation: 90,
            x_px: 30.0,
            y_px: 150.0,
        };
        assert_eq!(
            expand_launch(
                "{path}#{currentpage}/{currentpage0} {zoom}% {rotation} {x},{y} {xpx},{ypx}",
                &v
            ),
            "/tmp/a b.pdf#3/2 150% 90 20,100 30,150"
        );
    }
}
