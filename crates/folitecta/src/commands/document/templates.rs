//! New documents from templates (FOL-L010): the gallery of File › From
//! Template…, the pictures of its templates drawn away from the interface
//! thread, the words of the templates in the language of the interface.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use dioxus::prelude::*;
use folitecta_core::templates::WORDS;
use folitecta_core::{Document, Renderer, Template, TemplateOptions, UserTemplate};

use crate::commands::{handlers, no_doc};
use crate::files;
use crate::i18n::{Lang, t, tf};
use crate::state::{AppState, Dialog, ToastKind};

/// The pictures of the templates drawn so far, by [`picture_key`].
pub static PICTURES: GlobalSignal<HashMap<String, String>> = Signal::global(HashMap::new);

/// The pictures being drawn.
static DRAWING: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// How wide the pictures are drawn (pixels).
const PICTURE_WIDTH: f32 = 150.0;

/// The words of the templates in `lang` (English ones are the templates' own).
pub fn labels(lang: Lang) -> BTreeMap<String, String> {
    WORDS
        .iter()
        .filter_map(|(k, _)| {
            let key = format!("templates.word.{k}");
            let w = t(lang, &key);
            (w != key).then(|| (k.to_string(), w))
        })
        .collect()
}

/// What tells two pictures apart: the template, its paper, its month, its words.
pub fn picture_key(tpl: Template, o: &TemplateOptions, lang: Lang) -> String {
    format!(
        "{}:{}x{}:{}-{}:{}:{:?}",
        tpl.id(),
        o.width.round(),
        o.height.round(),
        o.year,
        o.month,
        o.monday_first,
        lang
    )
}

/// Draw the picture of `tpl` (its first page) unless it is drawn or being drawn.
pub fn want_picture(tpl: Template, o: &TemplateOptions, lang: Lang) {
    let key = picture_key(tpl, o, lang);
    if PICTURES.peek().contains_key(&key) {
        return;
    }
    {
        let mut drawing = DRAWING.lock().unwrap_or_else(|e| e.into_inner());
        if !drawing.get_or_insert_with(HashSet::new).insert(key.clone()) {
            return;
        }
    }
    let opts = TemplateOptions {
        pages: 1,
        ..o.clone()
    };
    spawn(async move {
        let png = tokio::task::spawn_blocking(move || {
            let doc = Document::from_template(tpl, &opts).ok()?;
            let scale = PICTURE_WIDTH / opts.width.max(1.0);
            Renderer::new(doc.bytes())
                .and_then(|r| r.render_page(0, scale))
                .ok()
                .map(|p| p.png)
        })
        .await
        .ok()
        .flatten();
        if let Some(png) = png {
            use base64::Engine;
            let url = format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png)
            );
            PICTURES.write().insert(key.clone(), url);
        }
        if let Some(d) = DRAWING.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            d.remove(&key);
        }
    });
}

/// Make the document of `tpl` (away from the interface) and open it in a tab.
pub fn create(st: AppState, tpl: Template, opts: TemplateOptions) {
    st.set_dialog(Dialog::None);
    st.set_value("templates.last", tpl.id());
    dioxus::core::spawn_forever(async move {
        match tokio::task::spawn_blocking(move || Document::from_template(tpl, &opts)).await {
            Ok(Ok(doc)) => files::open_document_tab(st, doc),
            Ok(Err(e)) => st.error(st.core_error_text(&e)),
            Err(_) => {}
        }
    });
}

/// The folder of the user's own templates.
pub fn templates_dir() -> Option<PathBuf> {
    crate::state::app_data_dir().map(|d| d.join("Templates"))
}

/// A template of the user: its file, what it says of itself, the picture
/// of its first page.
#[derive(Debug, Clone, PartialEq)]
pub struct MyTemplate {
    pub path: PathBuf,
    pub info: UserTemplate,
    pub picture: Option<String>,
}

/// The user's templates as last read (`None` until they are read).
pub static MINE: GlobalSignal<Option<Vec<MyTemplate>>> = Signal::global(|| None);

/// Read the user's templates again, away from the interface.
pub fn load_mine() {
    dioxus::core::spawn_forever(async move {
        let list = tokio::task::spawn_blocking(read_mine).await.unwrap_or_default();
        *MINE.write() = Some(list);
    });
}

/// The templates of the folder, by name, with their pictures; files that
/// are not templates are left out.
fn read_mine() -> Vec<MyTemplate> {
    let Some(entries) = templates_dir().and_then(|d| std::fs::read_dir(d).ok()) else {
        return Vec::new();
    };
    let mut out: Vec<MyTemplate> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pdf")))
        .filter_map(|path| {
            let doc = Document::from_bytes(std::fs::read(&path).ok()?, None, None).ok()?;
            let info = doc.template_info()?;
            let picture = doc.page_size(0).ok().and_then(|size| {
                let scale = PICTURE_WIDTH / size.width.max(1.0);
                Renderer::new(doc.bytes())
                    .and_then(|r| r.render_page(0, scale))
                    .ok()
            });
            let picture = picture.map(|p| {
                use base64::Engine;
                format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(p.png)
                )
            });
            Some(MyTemplate { path, info, picture })
        })
        .collect();
    out.sort_by_key(|m| m.info.name.to_lowercase());
    out
}

/// A file name for a template named `name` that is not taken in `dir`.
fn file_for(dir: &Path, name: &str) -> PathBuf {
    let mut base: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    base = base.trim_matches('-').to_string();
    if base.is_empty() {
        base = "template".into();
    }
    (1..)
        .map(|i| {
            if i == 1 {
                dir.join(format!("{base}.pdf"))
            } else {
                dir.join(format!("{base}-{i}.pdf"))
            }
        })
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join(format!("{base}.pdf")))
}

/// Whether a template of the user is called `name` (other than `except`).
fn name_used(name: &str, except: Option<&Path>) -> bool {
    MINE.peek().as_ref().is_some_and(|list| {
        list.iter()
            .any(|m| m.info.name.eq_ignore_ascii_case(name.trim()) && Some(m.path.as_path()) != except)
    })
}

/// Save the active document as a template named `name`.
pub fn save_as_template(st: AppState, name: String) {
    let lang = st.lang();
    if name_used(&name, None) {
        st.error(tf(lang, "templates.name_used", &[("name", name.trim())]));
        return;
    }
    let Some(dir) = templates_dir() else { return };
    let Some(made) = st.read_active(|t| t.doc.template_bytes(&name)) else {
        return;
    };
    let written = made.map_err(|e| st.core_error_text(&e)).and_then(|bytes| {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = file_for(&dir, &name);
        folitecta_core::util::write_atomic(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
    });
    match written {
        Ok(()) => {
            st.set_dialog(Dialog::None);
            st.toast(
                ToastKind::Ok,
                tf(lang, "templates.saved", &[("name", name.trim())]),
                "",
            );
            load_mine();
        }
        Err(e) => st.error(e),
    }
}

/// A new document from the template file `path` (in a tab of its own).
pub fn create_from_mine(st: AppState, path: PathBuf) {
    st.set_dialog(Dialog::None);
    dioxus::core::spawn_forever(async move {
        let made = tokio::task::spawn_blocking(move || {
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            Document::from_template_file(bytes).map_err(|e| e.to_string())
        })
        .await;
        match made {
            Ok(Ok(doc)) => files::open_document_tab(st, doc),
            Ok(Err(e)) => st.error(e),
            Err(_) => {}
        }
    });
}

/// Give a template of the user another name.
pub fn rename_mine(st: AppState, path: PathBuf, name: String) {
    if name.trim().is_empty() {
        return;
    }
    if name_used(&name, Some(&path)) {
        st.error(tf(st.lang(), "templates.name_used", &[("name", name.trim())]));
        return;
    }
    let r = std::fs::read(&path)
        .map_err(|e| e.to_string())
        .and_then(|b| folitecta_core::user_templates::renamed_template(b, &name).map_err(|e| e.to_string()))
        .and_then(|b| folitecta_core::util::write_atomic(&path, b).map_err(|e| e.to_string()));
    match r {
        Ok(()) => load_mine(),
        Err(e) => st.error(e),
    }
}

/// Move a template of the user to the bin.
pub fn remove_mine(st: AppState, path: PathBuf) {
    match crate::util::move_to_trash(&path) {
        Ok(()) => load_mine(),
        Err(e) => st.error(e),
    }
}

handlers! {
    "newDocTemplate" => |st| st.set_dialog(Dialog::Templates),
    "saveAsTemplate" => |st| if !no_doc(st) { st.set_dialog(Dialog::SaveTemplate) },
}
