//! Table Options (FOL-L005): a document made from a CSV file is made again
//! with the separator, the first row, the paper, the turn of the pages and the
//! size of the text chosen.

use dioxus::prelude::*;
use folitecta_core::csvdoc::{CsvOptions, TableTurn, csv_shape};
use folitecta_core::{Document, TextLayout};

use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, DocTab, ToastKind};
use crate::ui::dialogs::Frame;

/// Papers offered: id, width and height in points.
const PAPERS: [(&str, f32, f32); 4] = [
    ("A4", 595.28, 841.89),
    ("Letter", 612.0, 792.0),
    ("A3", 841.89, 1190.55),
    ("Legal", 612.0, 1008.0),
];

/// The choices of the window as options.
fn options(separator: &str, header: &str, turn: &str) -> CsvOptions {
    CsvOptions {
        separator: match separator {
            "comma" => Some(','),
            "semicolon" => Some(';'),
            "tab" => Some('\t'),
            "bar" => Some('|'),
            _ => None,
        },
        header: match header {
            "yes" => Some(true),
            "no" => Some(false),
            _ => None,
        },
        turn: match turn {
            "upright" => TableTurn::Upright,
            "sideways" => TableTurn::Sideways,
            _ => TableTurn::Auto,
        },
    }
}

/// Make the table of the active document again with `options`, on `paper`.
pub fn remake(st: AppState, opts: CsvOptions, paper: &str, size: f32) -> Result<(), String> {
    let Some((id, Some((text, name)))) = st.read_active(|t| (t.id, t.made_from.clone())) else {
        return Err(t(st.lang(), "toast.not_a_table"));
    };
    let (w, h) = PAPERS
        .iter()
        .find(|(p, _, _)| *p == paper)
        .map(|(_, w, h)| (*w, *h))
        .unwrap_or((PAPERS[0].1, PAPERS[0].2));
    let layout = TextLayout {
        font_size: size.clamp(5.0, 24.0),
        page_width: w,
        page_height: h,
        ..TextLayout::default()
    };
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(&name).to_string();
    let mut doc = Document::from_csv_with(&text, &opts, layout, &stem).map_err(|e| st.core_error_text(&e))?;
    let _ = doc.set_info(&stem, "", "", "");
    doc.set_path(None);
    doc.set_name_hint(&format!("{stem}.pdf"));
    doc.clear_history();
    let mut docs = st.docs_mut();
    if let Some(tab) = docs.iter_mut().find(|t| t.id == id) {
        let mut fresh = DocTab::new(id, doc);
        fresh.made_from = Some((text, name));
        *tab = fresh;
    }
    Ok(())
}

#[component]
pub fn CsvOptionsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let separator = use_signal(|| st.value("csv.separator"));
    let header = use_signal(|| st.value("csv.header"));
    let turn = use_signal(|| st.value("csv.turn"));
    let mut paper = use_signal(|| {
        let p = st.value("csv.paper");
        if p.is_empty() { "A4".to_string() } else { p }
    });
    let mut size = use_signal(|| {
        let s = st.value("csv.size");
        if s.is_empty() { "11".to_string() } else { s }
    });
    let text = st
        .read_active(|t| t.made_from.as_ref().map(|(text, _)| text.clone()))
        .flatten()
        .unwrap_or_default();
    let (rows, cols, _, has_header) = csv_shape(&text, &options(&separator(), &header(), &turn()));
    let shape = tf(
        lang,
        "dialog.csv.shape",
        &[
            ("rows", &rows.saturating_sub(usize::from(has_header)).to_string()),
            ("cols", &cols.to_string()),
        ],
    );
    let select = |key: &'static str, values: Vec<(&'static str, String)>, mut signal: Signal<String>| {
        rsx! {
            div { class: "field",
                label { "{t(lang, key)}" }
                select { class: "csv-{key.rsplit('.').next().unwrap_or_default()}", onchange: move |e| signal.set(e.value()),
                    for (v, label) in values {
                        option { key: "{v}", value: "{v}", selected: signal() == v, "{label}" }
                    }
                }
            }
        }
    };
    let opt = |v: &'static str, key: &str| (v, t(lang, key));
    rsx! {
        Frame { title: t(lang, "dialog.csv.title"),
            on_ok: move |_| {
                st.set_value("csv.separator", separator());
                st.set_value("csv.header", header());
                st.set_value("csv.turn", turn());
                st.set_value("csv.paper", paper());
                st.set_value("csv.size", size());
                let pts = size().trim().replace(',', ".").parse::<f32>().unwrap_or(11.0);
                match remake(st, options(&separator(), &header(), &turn()), &paper(), pts) {
                    Ok(()) => {
                        st.set_dialog(Dialog::None);
                        crate::files::set_window_title(st);
                        st.toast(ToastKind::Ok, t(st.lang(), "toast.table_made"), "");
                    }
                    Err(e) => st.error(e),
                }
            },
            p { class: "hint", "{t(lang, \"dialog.csv.hint\")}" }
            p { class: "muted csv-shape", "{shape}" }
            div { class: "field-grid",
                {select("dialog.csv.separator", vec![opt("", "dialog.csv.sep_auto"), opt("comma", "dialog.csv.sep_comma"), opt("semicolon", "dialog.csv.sep_semicolon"), opt("tab", "dialog.csv.sep_tab"), opt("bar", "dialog.csv.sep_bar")], separator)}
                {select("dialog.csv.header", vec![opt("", "dialog.csv.header_auto"), opt("yes", "dialog.csv.header_yes"), opt("no", "dialog.csv.header_no")], header)}
                div { class: "field",
                    label { "{t(lang, \"dialog.csv.paper\")}" }
                    select { class: "csv-paper", onchange: move |e| paper.set(e.value()),
                        for (p, _, _) in PAPERS {
                            option { key: "{p}", value: "{p}", selected: paper() == p, "{p}" }
                        }
                    }
                }
                {select("dialog.csv.turn", vec![opt("", "dialog.csv.turn_auto"), opt("upright", "dialog.csv.turn_upright"), opt("sideways", "dialog.csv.turn_sideways")], turn)}
                div { class: "field",
                    label { "{t(lang, \"dialog.csv.size\")}" }
                    input { class: "csv-size", r#type: "number", min: "5", max: "24", step: "0.5", value: "{size}", oninput: move |e| size.set(e.value()) }
                }
            }
        }
    }
}
