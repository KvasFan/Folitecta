//! How redacted areas look once applied, and the sets of redaction codes.

use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use folitecta_core::redaction::{
    OVERLAY_FONTS, Overlay, RedactionLook, RedactionOptions, SanitizeOptions, SanitizeReport,
};
use folitecta_core::registry;

use crate::commands::redaction::{
    CodeSet, code_sets, current_look, current_set, keep_look, redaction_method, sanitize_options,
    save_code_sets, save_copy_after_apply, save_sanitize_options, selected_mark, set_save_copy_after_apply,
};
use crate::i18n::{Lang, cmd_label, t, tf};
use crate::state::{AppState, Dialog};
use crate::ui::dialogs::Frame;
use crate::util::{color_hex, parse_color};

fn label_of(lang: Lang, id: &str) -> String {
    registry()
        .get(id)
        .map(|c| cmd_label(lang, c))
        .unwrap_or_else(|| id.to_string())
}

const OVERLAYS: &[(Overlay, &str)] = &[
    (Overlay::Standard, "redaction.overlay.standard"),
    (Overlay::Blur, "redaction.overlay.blur"),
    (Overlay::BlurExtra, "redaction.overlay.blurExtra"),
    (Overlay::Pixelate, "redaction.overlay.pixelate"),
    (Overlay::PixelateExtra, "redaction.overlay.pixelateExtra"),
];

#[component]
pub fn RedactionLookDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut look = use_signal(|| current_look(st));
    let sets = code_sets(st);
    let mut set = use_signal(|| current_set(st).min(sets.len().saturating_sub(1)));
    let on_mark = selected_mark(st).is_some();
    let l = look();
    let codes: Vec<String> = sets.get(set()).map(|s| s.codes.clone()).unwrap_or_default();
    let size_text = if l.font_size > 0.0 {
        l.font_size.to_string()
    } else {
        String::new()
    };
    let as_pictures = st.value("redact.method") == "picture";
    let mut sanitize = use_signal(|| sanitize_options(st));
    let mut save_copy = use_signal(|| save_copy_after_apply(st));
    // The sanitizing chosen is kept as it changes.
    use_effect(move || {
        let o = sanitize();
        save_sanitize_options(st, &o);
    });
    let _ = &mut sanitize;
    rsx! {
        Frame { title: label_of(lang, "document.redaction"),
            on_ok: move |_| {
                keep_look(st, &look());
                st.set_value("redact.codeset", set().to_string());
                st.set_dialog(Dialog::None);
            },
            p { class: "hint", if on_mark { "{t(lang, \"redact.for_mark\")}" } else { "{t(lang, \"redact.for_new\")}" } }
            div { class: "field", label { "{label_of(lang, \"redaction.overlayType\")}" }
                div { class: "radio-row",
                    for (o, id) in OVERLAYS.iter().copied() {
                        label { key: "{id}",
                            input { r#type: "radio", name: "redact-overlay", checked: l.overlay == o, onchange: move |_| look.write().overlay = o }
                            span { "{label_of(lang, id)}" }
                        }
                    }
                }
            }
            if l.overlay != Overlay::Standard {
                p { class: "hint", "{t(lang, \"redact.blur_hint\")}" }
            }
            div { class: "field-grid",
                div { class: "field", label { "{label_of(lang, \"overlay.fcolor\")}" }
                    input { r#type: "color", value: "{color_hex(l.fill)}", disabled: l.overlay != Overlay::Standard,
                        oninput: move |e| look.write().fill = parse_color(&e.value()) } }
                div { class: "field", label { "{label_of(lang, \"overlay.textColor\")}" }
                    input { r#type: "color", value: "{color_hex(l.text_color)}", oninput: move |e| look.write().text_color = parse_color(&e.value()) } }
            }
            div { class: "field", label { "{t(lang, \"redact.text\")}" }
                input { value: "{l.text}", placeholder: "{t(lang, \"redact.text_hint\")}", oninput: move |e| look.write().text = e.value() } }
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"redact.code_set\")}" }
                    select { onchange: move |e| set.set(e.value().parse().unwrap_or(0)),
                        if sets.is_empty() { option { value: "0", "—" } }
                        for (i, s) in sets.iter().enumerate() {
                            option { key: "{i}", value: "{i}", selected: i == set(), "{s.name}" }
                        }
                    }
                }
                div { class: "field", label { "{t(lang, \"redact.code\")}" }
                    select { onchange: move |e| look.write().text = e.value(),
                        option { value: "", "—" }
                        for c in codes.iter() {
                            option { key: "{c}", value: "{c}", selected: *c == l.text, "{c}" }
                        }
                    }
                }
            }
            div { class: "row",
                button { class: "tbtn", onclick: move |_| st.set_dialog(Dialog::RedactionCode { set: false }), "{label_of(lang, \"redaction.addTextCode\")}" }
                button { class: "tbtn", onclick: move |_| st.set_dialog(Dialog::RedactionCode { set: true }), "{label_of(lang, \"redaction.addTextCodeSet\")}" }
            }
            div { class: "field-grid",
                div { class: "field", label { "{label_of(lang, \"overlay.fontName\")}" }
                    select { onchange: move |e| look.write().font = e.value(),
                        for (name, _) in OVERLAY_FONTS.iter().copied() {
                            option { key: "{name}", value: "{name}", selected: name == l.font, "{name}" }
                        }
                    }
                }
                div { class: "field", label { "{label_of(lang, \"overlay.fontSize\")}" }
                    input { r#type: "number", min: "0", max: "200", step: "1", value: "{size_text}", placeholder: "{t(lang, \"redact.fit\")}",
                        oninput: move |e| look.write().font_size = e.value().replace(',', ".").parse::<f32>().unwrap_or(0.0).clamp(0.0, 200.0) } }
            }
            label { class: "check",
                input { r#type: "checkbox", checked: l.repeat, onchange: move |_| { let r = !look.peek().repeat; look.write().repeat = r; } }
                span { "{label_of(lang, \"overlay.repeatText\")}" }
            }
            // How the marks are applied (for every mark of the documents).
            label { class: "check",
                input { r#type: "checkbox", checked: as_pictures,
                    onchange: move |_| st.set_value("redact.method", if as_pictures { "" } else { "picture" }) }
                span { "{t(lang, \"redact.as_pictures\")}" }
            }
            p { class: "hint", "{t(lang, \"redact.method_hint\")}" }
            h4 { "{t(lang, \"redact.sanitize\")}" }
            SanitizeChecks { opts: sanitize, counts: None, counting: false }
            p { class: "hint", "{t(lang, \"redact.sanitize_hint\")}" }
            label { class: "check",
                input { r#type: "checkbox", checked: save_copy(),
                    onchange: move |_| {
                        let on = !*save_copy.peek();
                        save_copy.set(on);
                        set_save_copy_after_apply(st, on);
                    } }
                span { "{t(lang, \"redact.save_copy\")}" }
            }
        }
    }
}

/// The sanitizing parts as check boxes; `counts` (from a preview) after
/// each label, or "counting" while the preview runs.
#[component]
fn SanitizeChecks(opts: Signal<SanitizeOptions>, counts: Option<SanitizeReport>, counting: bool) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut opts = opts;
    let o = opts();
    type Get = fn(&SanitizeOptions) -> bool;
    type Set = fn(&mut SanitizeOptions, bool);
    let rows: [(&str, Get, Set, Option<usize>); 5] = [
        (
            "redact.sanitize.metadata",
            |o| o.metadata,
            |o, v| o.metadata = v,
            counts.as_ref().map(|c| usize::from(c.metadata)),
        ),
        (
            "redact.sanitize.attachments",
            |o| o.attachments,
            |o, v| o.attachments = v,
            counts.as_ref().map(|c| c.attachments),
        ),
        (
            "redact.sanitize.hidden",
            |o| o.hidden,
            |o, v| o.hidden = v,
            counts.as_ref().map(|c| c.hidden + c.hidden_layers),
        ),
        (
            "redact.sanitize.quoting",
            |o| o.quoting,
            |o, v| o.quoting = v,
            counts.as_ref().map(|c| c.quotes),
        ),
        (
            "redact.sanitize.scripts",
            |o| o.scripts,
            |o, v| o.scripts = v,
            counts.as_ref().map(|c| c.scripts),
        ),
    ];
    rsx! {
        div { class: "sanitize-checks",
            for (key, get, set, n) in rows {
                label { class: "check", key: "{key}",
                    input { r#type: "checkbox", checked: get(&o),
                        onchange: move |_| {
                            let v = !get(&opts.peek());
                            set(&mut opts.write(), v);
                        } }
                    span {
                        "{t(lang, key)}"
                        if let Some(n) = n { " ({n})" } else if counting { " ({t(lang, \"redact.counting\")})" }
                    }
                }
            }
        }
    }
}

/// What a preview of a redaction found: the words of the removed text and
/// what sanitizing would remove.
#[derive(Debug, Clone, PartialEq)]
struct Preview {
    terms: Vec<String>,
    counts: SanitizeReport,
    /// Type 3 fonts and patterns that other pages use and that would keep
    /// what is under the marks (found by applying the marks to a copy).
    kept: Option<(usize, usize)>,
}

/// The latest preview, with the request it answers (kept on the root, so a
/// closing dialog never leaves a task writing a dropped signal).
static PREVIEW: GlobalSignal<Option<(u64, Preview)>> = Signal::global(|| None);
static NEXT_PREVIEW: AtomicU64 = AtomicU64::new(1);

/// Start a preview of applying the marks of `page` (all pages when None)
/// away from the interface; returns its request number.
fn start_preview(st: AppState, page: Option<usize>) -> u64 {
    let request = NEXT_PREVIEW.fetch_add(1, Ordering::Relaxed);
    let Some(snapshot) = st.read_active(|t| t.doc.snapshot()) else {
        return request;
    };
    // Counted for every part, whatever is chosen now.
    let opts = SanitizeOptions {
        placeholder: t(st.lang(), "redact.placeholder"),
        ..SanitizeOptions::default()
    };
    let method = redaction_method(st);
    dioxus::core::spawn_forever(async move {
        let pages = page.map(|p| vec![p]);
        let found = tokio::task::spawn_blocking(move || {
            let doc = snapshot.open().ok()?;
            let (terms, counts) = doc.redaction_preview(pages.as_deref(), &opts);
            Some((
                doc,
                pages,
                Preview {
                    terms: terms.iter().map(|t| t.shown().to_string()).collect(),
                    counts,
                    kept: None,
                },
            ))
        })
        .await;
        let Ok(Some((mut doc, pages, preview))) = found else {
            return;
        };
        *PREVIEW.write() = Some((request, preview.clone()));
        // What would stay for other pages: the marks applied to the copy.
        let kept = tokio::task::spawn_blocking(move || {
            let opts = RedactionOptions {
                method,
                sanitize: SanitizeOptions::none(),
                beyond_crop: true,
            };
            doc.apply_redactions_opts(pages.as_deref(), &opts)
                .ok()
                .map(|r| (r.type3_kept, r.patterns_kept))
        })
        .await;
        if let Ok(kept) = kept {
            let still = PREVIEW.peek().as_ref().is_some_and(|(id, _)| *id == request);
            if still {
                *PREVIEW.write() = Some((request, Preview { kept, ..preview }));
            }
        }
    });
    request
}

/// Apply the redaction marks of a page (or of every page): what is removed,
/// the warnings, the sanitizing chosen (with what each part would remove)
/// and whether the result is saved as a new file and checked.
#[component]
pub fn ApplyRedactionsDialog(page: Option<usize>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let pages = page.map(|p| vec![p]);
    let summary = st
        .read_active(|t| t.doc.redaction_summary(pages.as_deref()))
        .unwrap_or_default();
    let opts = use_signal(|| sanitize_options(st));
    let mut save_copy = use_signal(|| save_copy_after_apply(st));
    let mut skipped = use_signal(Vec::<String>::new);
    let mut beyond = use_signal(|| true);
    let request = use_hook(|| start_preview(st, page));
    let preview = PREVIEW
        .read()
        .as_ref()
        .filter(|(id, _)| *id == request)
        .map(|(_, p)| p.clone());
    let counting = preview.is_none();
    let counts = preview.as_ref().map(|p| p.counts.clone());
    let terms = preview.as_ref().map(|p| p.terms.clone()).unwrap_or_default();
    let kept = preview.as_ref().and_then(|p| p.kept);
    let summary_text = tf(
        lang,
        "redact.apply_summary",
        &[
            ("areas", &summary.areas.to_string()),
            ("pages", &summary.pages.to_string()),
        ],
    );
    let title = registry()
        .get(if page.is_some() {
            "annots.applyRedactions"
        } else {
            "document.applyRedactions"
        })
        .map(|c| cmd_label(lang, c))
        .unwrap_or_else(|| t(lang, "dialog.redact.title"));
    rsx! {
        Frame { title, danger: true, wide: true, ok_label: t(lang, "redact.apply"),
            on_ok: move |_| {
                let mut sanitize = opts();
                save_sanitize_options(st, &sanitize);
                set_save_copy_after_apply(st, save_copy());
                sanitize.skip_terms = skipped().iter().map(|s| s.to_lowercase()).collect();
                let options = RedactionOptions { method: redaction_method(st), sanitize, beyond_crop: beyond() };
                st.set_dialog(Dialog::None);
                crate::commands::protect::apply_redactions_now(st, page.map(|p| vec![p]), options, save_copy());
            },
            p { class: "apply-summary", "{summary_text}" }
            p { class: "hint", "{t(lang, \"dialog.redact.body\")}" }
            p { class: "hint", "{t(lang, \"redact.width_hint\")}" }
            if summary.blurred > 0 {
                div { class: "callout warn", "{tf(lang, \"redact.apply_blurred\", &[(\"count\", &summary.blurred.to_string())])}" }
            }
            if summary.signatures > 0 {
                div { class: "callout warn", "{tf(lang, \"redact.apply_signatures\", &[(\"count\", &summary.signatures.to_string())])}" }
            }
            if summary.beyond_crop > 0 {
                label { class: "check redact-beyond-crop",
                    input { r#type: "checkbox", checked: beyond(),
                        onchange: move |_| { let on = !*beyond.peek(); beyond.set(on); } }
                    span { "{tf(lang, \"redact.beyond_crop\", &[(\"count\", &summary.beyond_crop.to_string())])}" }
                }
            }
            if let Some((type3, patterns)) = kept.filter(|(a, b)| a + b > 0) {
                div { class: "callout warn", "{tf(lang, \"redact.apply_kept\", &[(\"type3\", &type3.to_string()), (\"patterns\", &patterns.to_string())])}" }
            }
            h4 { "{t(lang, \"redact.sanitize_scope\")}" }
            SanitizeChecks { opts, counts, counting }
            if opts().quoting && !counting {
                if terms.is_empty() {
                    p { class: "hint", "{t(lang, \"redact.terms_none\")}" }
                } else {
                    p { class: "hint", "{t(lang, \"redact.terms\")}" }
                    div { class: "redaction-terms",
                        for term in terms.iter().cloned() {
                            label { class: "check", key: "{term}",
                                input { r#type: "checkbox", checked: !skipped().contains(&term),
                                    onchange: {
                                        let term = term.clone();
                                        move |_| {
                                            let mut list = skipped.peek().clone();
                                            match list.iter().position(|x| *x == term) {
                                                Some(i) => { list.remove(i); }
                                                None => list.push(term.clone()),
                                            }
                                            skipped.set(list);
                                        }
                                    } }
                                span { "{term}" }
                            }
                        }
                    }
                }
            }
            p { class: "hint", "{t(lang, \"redact.sanitize_hint\")}" }
            label { class: "check",
                input { r#type: "checkbox", checked: save_copy(),
                    onchange: move |_| { let on = !*save_copy.peek(); save_copy.set(on); } }
                span { "{t(lang, \"redact.save_copy\")}" }
            }
        }
    }
}

/// A new redaction code (in the set in use) or a new set of codes.
#[component]
pub fn RedactionCodeDialog(set: bool) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut value = use_signal(String::new);
    let title = label_of(
        lang,
        if set {
            "redaction.addTextCodeSet"
        } else {
            "redaction.addTextCode"
        },
    );
    rsx! {
        Frame { title,
            on_ok: move |_| {
                let v = value().trim().to_string();
                if v.is_empty() {
                    return;
                }
                let mut sets = code_sets(st);
                if set {
                    sets.push(CodeSet { name: v, codes: Vec::new() });
                    st.set_value("redact.codeset", (sets.len() - 1).to_string());
                } else {
                    if sets.is_empty() {
                        sets.push(CodeSet { name: t(st.lang(), "redact.codes"), codes: Vec::new() });
                    }
                    let i = current_set(st).min(sets.len() - 1);
                    if !sets[i].codes.contains(&v) {
                        sets[i].codes.push(v.clone());
                    }
                    // The new code is the overlay text from now on.
                    let mut look: RedactionLook = current_look(st);
                    look.text = v;
                    keep_look(st, &look);
                }
                save_code_sets(st, &sets);
                st.set_dialog(Dialog::RedactionLook);
            },
            div { class: "field",
                label { if set { "{t(lang, \"redact.set_name\")}" } else { "{t(lang, \"redact.code\")}" } }
                input { value: "{value}", autofocus: true, placeholder: if set { "FOIA" } else { "(b)(6)" }, oninput: move |e| value.set(e.value()) }
            }
        }
    }
}
