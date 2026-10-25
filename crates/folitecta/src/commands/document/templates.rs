//! New documents from templates (FOL-L010): the gallery of File › From
//! Template…, the pictures of its templates drawn away from the interface
//! thread, the words of the templates in the language of the interface.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

use dioxus::prelude::*;
use folitecta_core::templates::WORDS;
use folitecta_core::{Document, Renderer, Template, TemplateOptions};

use crate::commands::handlers;
use crate::files;
use crate::i18n::{Lang, t};
use crate::state::{AppState, Dialog};

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

handlers! {
    "newDocTemplate" => |st| st.set_dialog(Dialog::Templates),
}
