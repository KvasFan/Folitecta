//! JavaScript console, snippets and the named scripts of the document.
//!
//! Code runs in a Web Worker of the interface: a thread of its own without
//! access to the window, stopped when it takes too long. It sees a small
//! document API (`this.numPages`, `this.pageNum`, `this.getField(name).value`,
//! `this.info`, `app.alert`, `console.println`, `util.printf`); the changes it
//! asks for (field values, current page, document information, form reset)
//! are applied to the document afterwards.

use std::path::PathBuf;

use dioxus::prelude::*;
use serde_json::{Value, json};

use crate::commands::{handlers, no_doc};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind};

/// One line of the console output: kind (`in`, `out`, `alert`, `result`, `error`) and text.
pub type ConsoleLine = (String, String);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConsoleState {
    pub output: Vec<ConsoleLine>,
    pub running: bool,
    /// Selected snippet (file name without `.js`).
    pub snippet: Option<String>,
}

pub static CONSOLE: GlobalSignal<ConsoleState> = Signal::global(ConsoleState::default);

const RUNNER: &str = r#"
const payload = __PAYLOAD__;
const src = `self.onmessage = (e) => {
  const { code, ctx } = e.data;
  const out = [], ops = [];
  const str = (v) => { if (typeof v === 'string') return v; try { const j = JSON.stringify(v); return j === undefined ? String(v) : j; } catch (_) { return String(v); } };
  const console = {
    println: (s) => out.push(['out', String(s)]),
    log: (...a) => out.push(['out', a.map(str).join(' ')]),
    clear: () => ops.push({ op: 'clear' }),
    show() {}, hide() {},
  };
  const app = {
    alert: (m) => { const text = m && typeof m === 'object' ? m.cMsg : m; out.push(['alert', String(text)]); return 1; },
    beep() {}, response: () => null,
    viewerType: 'Folitecta', viewerVariation: 'Full', viewerVersion: ctx.version, platform: ctx.platform, language: ctx.lang,
  };
  const fields = ctx.fields;
  const names = Object.keys(fields);
  const field = (name) => {
    const f = fields[name];
    if (!f) return null;
    return {
      name, type: f.type, readonly: f.readonly, required: f.required,
      get value() { return f.value; },
      set value(v) { f.value = v == null ? '' : String(v); ops.push({ op: 'field', name, value: f.value }); },
      get valueAsString() { return String(f.value); },
    };
  };
  const info = new Proxy(Object.assign({}, ctx.info), {
    set(t, k, v) { t[k] = String(v); ops.push({ op: 'info', key: String(k), value: String(v) }); return true; },
  });
  const doc = {
    numPages: ctx.numPages,
    get pageNum() { return ctx.page; },
    set pageNum(n) { const p = Math.max(0, Math.min(ctx.numPages - 1, Math.trunc(Number(n) || 0))); ctx.page = p; ops.push({ op: 'page', page: p }); },
    numFields: names.length,
    getField: field,
    getNthFieldName: (i) => names[i] ?? null,
    info,
    get title() { return info.Title || ''; }, set title(v) { info.Title = v; },
    get author() { return info.Author || ''; }, set author(v) { info.Author = v; },
    get subject() { return info.Subject || ''; }, set subject(v) { info.Subject = v; },
    get keywords() { return info.Keywords || ''; }, set keywords(v) { info.Keywords = v; },
    documentFileName: ctx.fileName, path: ctx.path, filesize: ctx.size, dirty: ctx.dirty,
    resetForm() { ops.push({ op: 'reset' }); },
  };
  const util = {
    printf(fmt, ...args) {
      let i = 0;
      return String(fmt).replace(/%(\.(\d+))?([sdf%])/g, (m, _p, digits, k) => {
        if (k === '%') return '%';
        const v = args[i++];
        if (k === 's') return String(v);
        if (k === 'd') return String(Math.trunc(Number(v)));
        return Number(v).toFixed(digits === undefined ? 6 : Number(digits));
      });
    },
    printd: (fmt, d) => (d instanceof Date ? d : new Date(d)).toISOString().slice(0, 19).replace('T', ' '),
    scand: (fmt, s) => new Date(s),
  };
  const event = { value: '', rc: true };
  let result, ok = true, error = null;
  try {
    result = (function (app, console, util, event) { return eval(code); }).call(doc, app, console, util, event);
  } catch (err) {
    ok = false;
    error = err && err.name ? err.name + ': ' + err.message : String(err);
  }
  self.postMessage({ ok, error, result: result === undefined ? null : str(result), out, ops });
};`;
const url = URL.createObjectURL(new Blob([src], { type: 'text/javascript' }));
return await new Promise((resolve) => {
  let w;
  try { w = new Worker(url); } catch (err) { resolve({ ok: false, error: String(err), out: [], ops: [] }); return; }
  const done = (r) => { clearTimeout(timer); w.terminate(); URL.revokeObjectURL(url); resolve(r); };
  const timer = setTimeout(() => done({ ok: false, error: 'timeout', out: [], ops: [] }), payload.timeout);
  w.onmessage = (e) => done(e.data);
  w.onerror = (e) => { e.preventDefault(); done({ ok: false, error: e.message || 'error', out: [], ops: [] }); };
  w.postMessage({ code: payload.code, ctx: payload.ctx });
});
"#;

/// Field types as scripts name them.
fn kind_code(k: &folitecta_core::FieldKind) -> &'static str {
    use folitecta_core::FieldKind as K;
    match k {
        K::Text => "text",
        K::CheckBox => "checkbox",
        K::RadioButton => "radiobutton",
        K::ComboBox => "combobox",
        K::ListBox => "listbox",
        K::PushButton => "button",
        K::Signature => "signature",
        K::Unknown(_) => "unknown",
    }
}

/// What the script sees of the active document.
fn context(st: AppState) -> Value {
    let page = st.current_page();
    st.read_active(|t| {
        let info = t.doc.info();
        let mut fields = serde_json::Map::new();
        for f in t.doc.form_fields() {
            fields.insert(
                f.name.clone(),
                json!({
                    "value": f.value,
                    "type": kind_code(&f.kind),
                    "readonly": f.read_only,
                    "required": f.required,
                }),
            );
        }
        let path = t.doc.path().map(|p| p.display().to_string()).unwrap_or_default();
        json!({
            "numPages": t.doc.page_count(),
            "page": page,
            "fields": fields,
            "info": {
                "Title": info.title, "Author": info.author, "Subject": info.subject, "Keywords": info.keywords,
                "Creator": info.creator, "Producer": info.producer,
                "CreationDate": info.created, "ModDate": info.modified,
            },
            "fileName": t.doc.display_name(),
            "path": path,
            "size": info.file_size,
            "dirty": t.doc.is_dirty(),
            "version": env!("CARGO_PKG_VERSION"),
            "platform": std::env::consts::OS,
            "lang": st.lang().code(),
        })
    })
    .unwrap_or_else(|| json!({ "numPages": 0, "page": 0, "fields": {}, "info": {} }))
}

pub fn timeout_secs(st: AppState) -> u64 {
    st.value("js.timeout")
        .parse::<u64>()
        .ok()
        .filter(|s| (1..=60).contains(s))
        .unwrap_or(3)
}

/// Scripts may change the document (on unless turned off).
pub fn changes_allowed(st: AppState) -> bool {
    st.value("js.changes") != "0"
}

fn push(kind: &str, text: impl Into<String>) {
    CONSOLE.write().output.push((kind.to_string(), text.into()));
}

/// Apply the changes a script asked for; returns how many were applied.
fn apply_ops(st: AppState, ops: &[Value]) -> usize {
    let mut applied = 0;
    let mut info_changes: Vec<(String, String)> = Vec::new();
    for op in ops {
        match op["op"].as_str().unwrap_or("") {
            "clear" => CONSOLE.write().output.clear(),
            "field" => {
                let (name, value) = (
                    op["name"].as_str().unwrap_or(""),
                    op["value"].as_str().unwrap_or(""),
                );
                let r = st.with_active(|t| {
                    let id = t
                        .doc
                        .form_fields()
                        .into_iter()
                        .find(|f| f.name == name)
                        .map(|f| f.id)?;
                    Some(t.doc.set_field_value(id, value))
                });
                match r.flatten() {
                    Some(Ok(())) => applied += 1,
                    Some(Err(e)) => push("error", e.to_string()),
                    None => {}
                }
            }
            "page" => {
                if let Some(p) = op["page"].as_u64() {
                    st.navigate(p as usize, true);
                    applied += 1;
                }
            }
            "info" => {
                let key = op["key"].as_str().unwrap_or("").to_string();
                if matches!(key.as_str(), "Title" | "Author" | "Subject" | "Keywords") {
                    info_changes.retain(|(k, _)| *k != key);
                    info_changes.push((key, op["value"].as_str().unwrap_or("").to_string()));
                }
            }
            "reset" => {
                if let Some(Ok(_)) = st.with_active(|t| t.doc.reset_form()) {
                    applied += 1;
                }
            }
            _ => {}
        }
    }
    if !info_changes.is_empty() {
        let r = st.with_active(|t| {
            let i = t.doc.info();
            let get = |k: &str, cur: String| {
                info_changes
                    .iter()
                    .find(|(x, _)| x == k)
                    .map(|(_, v)| v.clone())
                    .unwrap_or(cur)
            };
            t.doc.set_info(
                &get("Title", i.title),
                &get("Author", i.author),
                &get("Subject", i.subject),
                &get("Keywords", i.keywords),
            )
        });
        if let Some(Ok(())) = r {
            applied += info_changes.len();
        }
    }
    if applied > 0 {
        crate::files::set_window_title(st);
    }
    applied
}

/// Run code and write what it prints, returns or fails with in the console.
pub fn run(st: AppState, code: String) {
    if code.trim().is_empty() || CONSOLE.peek().running {
        return;
    }
    let payload = json!({
        "code": code,
        "ctx": context(st),
        "timeout": timeout_secs(st) * 1000,
    });
    let js = RUNNER.replace("__PAYLOAD__", &payload.to_string());
    let shown = code.trim().to_string();
    {
        let mut c = CONSOLE.write();
        c.running = true;
        c.output.push(("in".into(), shown));
    }
    spawn(async move {
        let r = document::eval(&js).await.unwrap_or(Value::Null);
        CONSOLE.write().running = false;
        for line in r["out"].as_array().cloned().unwrap_or_default() {
            let kind = line[0].as_str().unwrap_or("out").to_string();
            let text = line[1].as_str().unwrap_or("").to_string();
            if kind == "alert" && st.value("js.alerts") != "0" {
                st.toast(ToastKind::Info, text.clone(), "");
            }
            push(&kind, text);
        }
        if r["ok"].as_bool() == Some(true) {
            let ops = r["ops"].as_array().cloned().unwrap_or_default();
            if !ops.is_empty() {
                if changes_allowed(st) && st.has_doc() {
                    let n = apply_ops(st, &ops);
                    if n > 0 {
                        push("out", tf(st.lang(), "js.applied", &[("count", &n.to_string())]));
                    }
                } else if ops.iter().any(|o| o["op"] != "clear") {
                    push("error", t(st.lang(), "js.changes_off"));
                }
                if ops.iter().any(|o| o["op"] == "clear") && !changes_allowed(st) {
                    CONSOLE.write().output.clear();
                }
            }
            if let Some(res) = r["result"].as_str() {
                push("result", res);
            }
        } else {
            let err = r["error"].as_str().unwrap_or("error").to_string();
            let err = if err == "timeout" {
                tf(
                    st.lang(),
                    "js.timeout_msg",
                    &[("secs", &timeout_secs(st).to_string())],
                )
            } else {
                err
            };
            push("error", err);
        }
    });
}

/// Code of the console editor.
pub fn console_code(st: AppState) -> String {
    st.value("js.code")
}

fn show_console(st: AppState) {
    st.set_dialog(Dialog::JsConsole);
}

fn run_all(st: AppState) {
    if (st.dialog)() != Dialog::JsConsole {
        show_console(st);
        return;
    }
    run(st, console_code(st));
}

fn run_selection(st: AppState) {
    if (st.dialog)() != Dialog::JsConsole {
        show_console(st);
        return;
    }
    spawn(async move {
        let sel = document::eval(
            "const a = document.getElementById('js-code'); return a ? a.value.substring(a.selectionStart, a.selectionEnd) : '';",
        )
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
        if sel.trim().is_empty() {
            st.toast(ToastKind::Info, t(st.lang(), "js.select_code"), "");
        } else {
            run(st, sel);
        }
    });
}

// ---- Snippets ------------------------------------------------------------------------

pub fn snippets_dir() -> Option<PathBuf> {
    crate::state::app_data_dir().map(|d| d.join("snippets"))
}

/// Snippets: name and code, by name.
pub fn snippets() -> Vec<(String, String)> {
    let Some(dir) = snippets_dir() else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("js"))
        .filter_map(|e| {
            let name = e.path().file_stem()?.to_string_lossy().to_string();
            let code = std::fs::read_to_string(e.path()).ok()?;
            Some((name, code))
        })
        .collect();
    out.sort_by_key(|(n, _)| n.to_lowercase());
    out
}

fn snippet_path(name: &str) -> Option<PathBuf> {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " -_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let clean = clean.trim().trim_matches('.').to_string();
    (!clean.is_empty())
        .then(|| snippets_dir().map(|d| d.join(format!("{clean}.js"))))
        .flatten()
}

/// A free name: `base`, `base 2`, `base 3`...
fn free_name(base: &str) -> String {
    let taken: Vec<String> = snippets().into_iter().map(|(n, _)| n.to_lowercase()).collect();
    let mut name = base.to_string();
    let mut n = 1;
    while taken.contains(&name.to_lowercase()) {
        n += 1;
        name = format!("{base} {n}");
    }
    name
}

pub fn save_snippet(name: &str, code: &str) -> Result<String, String> {
    let path = snippet_path(name).ok_or("bad name")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, code).map_err(|e| e.to_string())?;
    Ok(path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default())
}

fn add_snippet(st: AppState) {
    let code = console_code(st);
    if code.trim().is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "js.empty_code"), "");
        return;
    }
    // The first comment line names the snippet.
    let base = code
        .lines()
        .find_map(|l| l.trim().strip_prefix("//").map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty() && s.chars().count() <= 40)
        .unwrap_or_else(|| t(st.lang(), "js.snippet"));
    match save_snippet(&free_name(&base), &code) {
        Ok(name) => {
            CONSOLE.write().snippet = Some(name.clone());
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "js.snippet_saved", &[("name", &name)]),
                "",
            );
        }
        Err(e) => st.error(e),
    }
}

fn selected_snippet(st: AppState) -> Option<(String, String)> {
    let name = CONSOLE.peek().snippet.clone();
    let found = name.and_then(|n| snippets().into_iter().find(|(s, _)| *s == n));
    if found.is_none() {
        st.toast(ToastKind::Info, t(st.lang(), "js.select_snippet"), "");
    }
    found
}

fn clone_snippet(st: AppState) {
    let Some((name, code)) = selected_snippet(st) else {
        return;
    };
    match save_snippet(&free_name(&name), &code) {
        Ok(n) => CONSOLE.write().snippet = Some(n),
        Err(e) => st.error(e),
    }
}

fn delete_snippet(st: AppState) {
    let Some((name, _)) = selected_snippet(st) else {
        return;
    };
    if let Some(p) = snippet_path(&name) {
        if let Err(e) = std::fs::remove_file(p) {
            st.error(e);
            return;
        }
    }
    CONSOLE.write().snippet = None;
}

/// Put the selected snippet in the console editor.
pub fn edit_snippet(st: AppState) {
    let Some((_, code)) = selected_snippet(st) else {
        return;
    };
    st.set_value("js.code", code);
    if (st.dialog)() != Dialog::JsConsole {
        show_console(st);
    }
}

fn open_folder(st: AppState) {
    let Some(dir) = snippets_dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    if let Err(e) = crate::util::open_external(&dir.display().to_string()) {
        st.error(e);
    }
}

handlers! {
    "jsConsole.show" => show_console,
    "jsConsole.run" => run_all,
    "jsConsole.runSelect" => run_selection,
    "jsConsole.clear" => |_| CONSOLE.write().output.clear(),
    "jsConsole.openFolder" => open_folder,
    "jsConsole.prefs" => |st| st.set_dialog(Dialog::JsPrefs),
    "view.javaScriptOpts" => |st| st.set_dialog(Dialog::JsPrefs),
    "jssnippet.add" => add_snippet,
    "jssnippet.clone" => clone_snippet,
    "jssnippet.delete" => delete_snippet,
    "jsEditSnippets" => edit_snippet,
    "javaScriptList.add" => |st| if !no_doc(st) { st.set_dialog(Dialog::DocScripts(Some(String::new()))) },
    "javaScriptList.edit" => |st| if !no_doc(st) { st.set_dialog(Dialog::DocScripts(None)) },
    "javaScriptList.delete" => |st| if !no_doc(st) { st.set_dialog(Dialog::DocScripts(None)) },
    "document.javaScript" => |st| if !no_doc(st) { st.set_dialog(Dialog::DocScripts(None)) },
    "javaScript.editAll" => |st| if !no_doc(st) { st.set_dialog(Dialog::DocScripts(None)) },
}
