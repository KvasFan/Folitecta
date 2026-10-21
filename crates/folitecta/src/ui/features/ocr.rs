//! Text recognition: the dialog that reads pages (which pages, how
//! carefully, leaving out pages that have text) with its progress, the
//! dialog with the text read in an area, and the review of the words read
//! with doubt.

use dioxus::prelude::*;
use folitecta_core::{DoubtfulWord, registry};

use crate::commands::ocr::{self, JOB, Quality};
use crate::i18n::{cmd_label, t, tf};
use crate::state::{AppState, Dialog};
use crate::ui::dialogs::Frame;
use crate::util::parse_page_range;

#[component]
pub fn OcrDialog(pictures_only: bool) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let id = if pictures_only {
        "document.ocrImages"
    } else {
        "document.OCRPages"
    };
    let title = registry()
        .get(id)
        .map(|c| cmd_label(lang, c).trim_end_matches('…').to_string())
        .unwrap_or_default();
    let n = st.page_count();
    let mut which = use_signal(|| "all".to_string());
    let mut range = use_signal(move || format!("1-{}", n.max(1)));
    let mut quality = use_signal(|| Quality::from_id(&st.value("ocr.quality")));
    let mut skip = use_signal(|| st.value("ocr.skipText") != "0");
    let mut review = use_signal(|| st.value("ocr.review") != "0");
    let has_models = use_hook(|| ocr::models().is_some());
    let job = JOB.read().clone();
    let pages = move || -> Result<Vec<usize>, String> {
        match which().as_str() {
            "current" => Ok(vec![st.current_page()]),
            "range" => parse_page_range(&range(), n),
            _ => Ok((0..n).collect()),
        }
    };
    let problem = pages().err();
    let start = move |_| {
        if JOB.peek().running || !has_models {
            return;
        }
        match pages() {
            Ok(list) => {
                st.set_value("ocr.quality", quality().id());
                st.set_value("ocr.skipText", if skip() { "1" } else { "0" });
                st.set_value("ocr.review", if review() { "1" } else { "0" });
                ocr::run(st, list, quality(), skip(), pictures_only);
            }
            Err(e) => st.error(e),
        }
    };
    let percent = (job.done * 100).checked_div(job.total).unwrap_or(0);
    let qualities: Vec<(Quality, String)> = Quality::ALL
        .into_iter()
        .map(|q| (q, t(lang, &format!("ocr.quality_{}", q.id()))))
        .collect();
    let progress = tf(
        lang,
        "ocr.progress",
        &[
            ("page", &(job.done + 1).min(job.total).to_string()),
            ("count", &job.total.to_string()),
        ],
    );
    rsx! {
        Frame { title, ok_label: Some(t(lang, "ocr.start")), on_ok: start,
            p { class: "hint",
                if pictures_only { "{t(lang, \"ocr.hint_pictures\")}" } else { "{t(lang, \"ocr.hint\")}" }
            }
            div { class: "field", label { "{t(lang, \"ocr.pages\")}" }
                div { class: "radio-row",
                    label { input { r#type: "radio", name: "ocr-pages", checked: which() == "all", onchange: move |_| which.set("all".into()) }
                        span { "{t(lang, \"ocr.all_pages\")}" } }
                    label { input { r#type: "radio", name: "ocr-pages", checked: which() == "current", onchange: move |_| which.set("current".into()) }
                        span { "{t(lang, \"ocr.current_page\")}" } }
                    label { input { r#type: "radio", name: "ocr-pages", class: "ocr-range-radio", checked: which() == "range", onchange: move |_| which.set("range".into()) }
                        span { "{t(lang, \"ocr.page_range\")}" } }
                }
                if which() == "range" {
                    input { class: "ocr-range", value: "{range}", oninput: move |e| range.set(e.value()) }
                }
                if let Some(p) = problem {
                    p { class: "hint warn", "{p}" }
                }
            }
            div { class: "field", label { "{t(lang, \"ocr.quality\")}" }
                select { class: "ocr-quality",
                    onchange: move |e| quality.set(Quality::from_id(&e.value())),
                    for (q, name) in qualities {
                        option { key: "{q.id()}", value: "{q.id()}", selected: quality() == q, "{name}" }
                    }
                }
            }
            if !pictures_only {
                label { class: "check",
                    input { r#type: "checkbox", checked: skip(), onchange: move |_| skip.toggle() }
                    span { "{t(lang, \"ocr.skip_text\")}" }
                }
            }
            label { class: "check",
                input { r#type: "checkbox", class: "ocr-review-after", checked: review(), onchange: move |_| review.toggle() }
                span { "{t(lang, \"ocr.review_after\")}" }
            }
            if job.running {
                div { class: "ocr-progress",
                    div { class: "ocr-bar", div { class: "ocr-fill", style: "width:{percent}%" } }
                    span { class: "muted", "{progress}" }
                    button { class: "tbtn ocr-stop", onclick: move |_| ocr::stop(), "{t(lang, \"ocr.stop\")}" }
                }
            }
            if !has_models {
                p { class: "hint warn", "{t(lang, \"ocr.no_models\")}" }
            }
        }
    }
}

#[component]
pub fn OcrTextDialog(text: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let copy = text.clone();
    rsx! {
        Frame { title: t(lang, "ocr.text_title"), wide: true,
            textarea { class: "tall ocr-text", value: "{text}", readonly: true }
            div { class: "row",
                button { class: "tbtn primary ocr-copy",
                    onclick: move |_| {
                        crate::util::copy_to_clipboard(&copy);
                        st.toast(crate::state::ToastKind::Ok, t(st.lang(), "toast.copied"), "");
                    },
                    "{t(lang, \"ocr.copy\")}" }
                button { class: "tbtn ocr-write", onclick: move |_| ocr::write_area(st), "{t(lang, \"ocr.write\")}" }
            }
            p { class: "hint", "{t(lang, \"ocr.write_hint\")}" }
        }
    }
}

/// The doubtful words of text recognition, one at a time (FOL-L014): the
/// picture of the word and its text, to correct, keep or take away. Words
/// skipped stay doubtful; `skipped` of them come before the one shown.
#[component]
pub fn OcrReviewDialog(skipped: usize) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let title = registry()
        .get("document.ocrReview")
        .map(|c| cmd_label(lang, c).trim_end_matches('…').to_string())
        .unwrap_or_default();
    // Read again when the document changes (not while typing).
    let words = use_memo(move || ocr::doubtful_words(st));
    let list = words.read();
    let Some(word) = list.get(skipped).cloned() else {
        let message = if skipped > 0 {
            t(lang, "ocr.review_skipped")
        } else {
            t(lang, "ocr.review_none")
        };
        return rsx! {
            Frame { title, side: true,
                p { class: "ocr-review-status", "{message}" }
            }
        };
    };
    let count = list.len();
    // Made again for every word (a key counts in a list): its text field,
    // picture and mark on the page are those of the word.
    rsx! {
        for word in std::iter::once(word) {
            ReviewWord {
                key: "{word.page}-{word.index}-{word.text}-{count}",
                title: title.clone(),
                word,
                position: skipped,
                count,
            }
        }
    }
}

#[component]
fn ReviewWord(title: String, word: DoubtfulWord, position: usize, count: usize) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut text = use_signal(|| word.text.clone());
    let picture = {
        let word = word.clone();
        use_resource(move || {
            let word = word.clone();
            async move { ocr::word_picture(st, &word).await }
        })
    };
    // The word on its page, beside the dialog (after this render: showing
    // it changes the state of the window).
    use_hook({
        let word = word.clone();
        move || {
            spawn(async move { ocr::reveal_word(st, &word) });
        }
    });
    let (page, index, read) = (word.page, word.index, word.text.clone());
    let correct = move |_| {
        let typed = text();
        let change = (typed.trim() != read.trim()).then_some(typed);
        ocr::review(st, page, index, change.as_deref());
    };
    let counter = tf(
        lang,
        "ocr.review_count",
        &[
            ("n", &(position + 1).to_string()),
            ("count", &count.to_string()),
            ("page", &(page + 1).to_string()),
        ],
    );
    let sure = tf(
        lang,
        "ocr.review_confidence",
        &[("percent", &format!("{:.0}", word.confidence * 100.0))],
    );
    rsx! {
        Frame { title, side: true, ok_label: Some(t(lang, "ocr.review_correct")), on_ok: correct,
            div { class: "ocr-review-head",
                span { class: "muted ocr-review-count", "{counter}" }
                span { class: "grow" }
                span { class: "muted", "{sure}" }
            }
            div { class: "ocr-review-picture",
                match picture.read().as_ref() {
                    Some(Some(url)) => rsx! { img { src: "{url}", alt: "{word.text}" } },
                    Some(None) => rsx! { span { class: "muted", "—" } },
                    None => rsx! { span { class: "muted", "…" } },
                }
            }
            div { class: "field", label { "{t(lang, \"ocr.review_text\")}" }
                input { class: "ocr-review-text", value: "{text}", spellcheck: "false", autofocus: true,
                    oninput: move |e| text.set(e.value()) }
            }
            p { class: "hint", "{t(lang, \"ocr.review_hint\")}" }
            div { class: "ocr-review-actions",
                button { class: "tbtn ocr-review-keep",
                    onclick: move |_| ocr::review(st, page, index, None), "{t(lang, \"ocr.review_keep\")}" }
                button { class: "tbtn ocr-review-remove",
                    onclick: move |_| ocr::review(st, page, index, Some("")), "{t(lang, \"ocr.review_remove\")}" }
                button { class: "tbtn ocr-review-skip",
                    onclick: move |_| st.set_dialog(Dialog::OcrReview { skipped: position + 1 }),
                    "{t(lang, \"ocr.review_skip\")}" }
            }
        }
    }
}
