//! The Spell Check and Replace dialog: the word the dictionaries do not
//! know with the text around it, suggestions and what to write instead, and
//! the options of the check (what is checked, languages, the user's words).

use dioxus::prelude::*;
use folitecta_core::registry;

use crate::commands::spelling::{self as sp, CHANGE_TO, SPELL, SUGGESTIONS};
use crate::i18n::{cmd_label, t, tf};
use crate::state::AppState;
use crate::ui::dialogs::Frame;

/// What is typed in "Change to" (a copy: the correction changes it).
fn typed() -> String {
    CHANGE_TO.peek().clone()
}

#[component]
pub fn SpellCheckDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let title = registry()
        .get("document.spellCheckAndRepl")
        .map(|c| cmd_label(lang, c).trim_end_matches('…').to_string())
        .unwrap_or_default();
    let mut show_options = use_signal(|| false);
    // The dictionaries in the folders, listed once for the dialog.
    let available = use_hook(sp::available);

    let status = sp::status_line(st);
    let shown = st
        .read_active(|t| {
            let run = t.spell.as_ref()?;
            let m = sp::current(t)?.clone();
            Some((
                sp::place_label(t, lang, &m),
                sp::context(t, &m),
                tf(
                    lang,
                    "spell.count",
                    &[
                        ("current", &(run.current.unwrap_or(0) + 1).to_string()),
                        ("count", &run.list.len().to_string()),
                    ],
                ),
                m.word,
            ))
        })
        .flatten();
    let none_left = st
        .read_active(|t| {
            t.spell
                .as_ref()
                .is_some_and(|r| r.made_for.is_some() && r.list.is_empty())
        })
        .unwrap_or(false);
    let has_word = shown.is_some();
    let suggestions = SUGGESTIONS.read().clone();
    let change_to = CHANGE_TO.read().clone();
    let scope = sp::scope(st);
    let opts = sp::options(st);
    let chosen = sp::chosen(st);
    let (added, missing) = {
        let s = SPELL.read();
        (
            s.speller.as_ref().map(|s| s.added_words()).unwrap_or_default(),
            s.missing.clone(),
        )
    };
    let missing_text = missing
        .iter()
        .map(|(code, why)| match why {
            Some(why) => format!("{}: {why}", sp::language_name(code)),
            None => tf(lang, "spell.not_found", &[("lang", &sp::language_name(code))]),
        })
        .collect::<Vec<_>>()
        .join(" · ");
    let folder = sp::user_dir()
        .map(|d| d.display().to_string())
        .unwrap_or_default();

    rsx! {
        Frame { title, side: true,
            if let Some((place, context, count, word)) = shown {
                div { class: "spell-head",
                    span { class: "muted", "{t(lang, \"spell.not_known\")}" }
                    span { class: "grow" }
                    span { class: "muted spell-count", "{count}" }
                }
                div { class: "spell-context",
                    if let Some((before, w, after)) = context {
                        span { "{before}" }
                        mark { "{w}" }
                        span { "{after}" }
                    } else {
                        mark { "{word}" }
                    }
                }
                div { class: "muted spell-place", "{place}" }
                div { class: "field", label { "{t(lang, \"spell.change_to\")}" }
                    input { class: "spell-to", value: "{change_to}", spellcheck: "false",
                        oninput: move |e| *CHANGE_TO.write() = e.value(),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter {
                                e.stop_propagation();
                                sp::change(st, &typed());
                            }
                        },
                    }
                }
                div { class: "field", label { "{t(lang, \"spell.suggestions\")}" }
                    div { class: "spell-suggestions",
                        match suggestions.as_ref().filter(|(w, _)| *w == word).map(|(_, l)| l.clone()) {
                            Some(Some(list)) if list.is_empty() => rsx! { span { class: "muted", "{t(lang, \"spell.no_suggestions\")}" } },
                            Some(Some(list)) => rsx! {
                                for s in list {
                                    {
                                        let on = s == change_to;
                                        let pick = s.clone();
                                        let use_it = s.clone();
                                        rsx! {
                                            button { key: "{s}", class: if on { "spell-sugg on" } else { "spell-sugg" },
                                                onclick: move |_| *CHANGE_TO.write() = pick.clone(),
                                                ondoubleclick: move |_| sp::change(st, &use_it),
                                                "{s}"
                                            }
                                        }
                                    }
                                }
                            },
                            _ => rsx! { span { class: "muted", "{t(lang, \"spell.finding\")}" } },
                        }
                    }
                }
            } else if let Some(line) = status {
                p { class: "spell-status", "{line}" }
            } else if none_left {
                p { class: "spell-status ok", "{t(lang, \"spell.none\")}" }
            }
            div { class: "spell-actions",
                button { class: "tbtn primary spell-change", disabled: !has_word,
                    onclick: move |_| sp::change(st, &typed()), "{t(lang, \"spell.change\")}" }
                button { class: "tbtn spell-change-all", disabled: !has_word,
                    onclick: move |_| sp::change_all(st, &typed()), "{t(lang, \"spell.change_all\")}" }
                button { class: "tbtn spell-ignore", disabled: !has_word,
                    onclick: move |_| sp::ignore(st), "{t(lang, \"spell.ignore\")}" }
                button { class: "tbtn spell-ignore-all", disabled: !has_word,
                    onclick: move |_| sp::ignore_all(st), "{t(lang, \"spell.ignore_all\")}" }
                button { class: "tbtn spell-add", disabled: !has_word,
                    onclick: move |_| sp::add_word(st), "{t(lang, \"spell.add\")}" }
            }
            div { class: "spell-nav",
                button { class: "tbtn spell-prev", disabled: !has_word,
                    onclick: move |_| crate::commands::dispatch(st, "document.spellCheckAndRepl.back"),
                    "{t(lang, \"spell.previous\")}" }
                button { class: "tbtn spell-next", disabled: !has_word,
                    onclick: move |_| crate::commands::dispatch(st, "document.spellCheckAndRepl.forward"),
                    "{t(lang, \"spell.next\")}" }
            }
            button { class: if show_options() { "tbtn toggle on spell-options-btn" } else { "tbtn toggle spell-options-btn" },
                onclick: move |_| show_options.toggle(), "{t(lang, \"spell.options\")}" }
            if show_options() {
                div { class: "spell-options",
                    div { class: "field", label { "{t(lang, \"spell.check_in\")}" }
                        div { class: "check-row",
                            label { class: "check",
                                input { r#type: "checkbox", class: "spell-in-pages", checked: scope.page_text,
                                    onchange: move |_| sp::set_flag(st, "spell.pages", !scope.page_text) }
                                span { "{t(lang, \"spell.page_text\")}" }
                            }
                            label { class: "check",
                                input { r#type: "checkbox", class: "spell-in-comments", checked: scope.comments,
                                    onchange: move |_| sp::set_flag(st, "spell.comments", !scope.comments) }
                                span { "{t(lang, \"spell.comments\")}" }
                            }
                            label { class: "check",
                                input { r#type: "checkbox", class: "spell-in-fields", checked: scope.fields,
                                    onchange: move |_| sp::set_flag(st, "spell.fields", !scope.fields) }
                                span { "{t(lang, \"spell.fields\")}" }
                            }
                        }
                    }
                    div { class: "field", label { "{t(lang, \"spell.languages\")}" }
                        div { class: "check-row spell-langs",
                            for f in available.iter() {
                                {
                                    let code = f.code.clone();
                                    let on = chosen.iter().any(|c| c.eq_ignore_ascii_case(&code));
                                    rsx! {
                                        label { key: "{code}", class: "check",
                                            input { r#type: "checkbox", checked: on,
                                                onchange: move |_| sp::choose_language(st, &code, !on) }
                                            span { "{sp::language_name(&f.code)}" }
                                        }
                                    }
                                }
                            }
                        }
                        if available.is_empty() {
                            p { class: "hint warn", "{t(lang, \"spell.no_dictionary\")}" }
                        }
                        if !missing_text.is_empty() {
                            p { class: "hint warn", "{missing_text}" }
                        }
                    }
                    div { class: "check-row",
                        label { class: "check",
                            input { r#type: "checkbox", checked: opts.skip_capitals,
                                onchange: move |_| sp::set_flag(st, "spell.capitals", !opts.skip_capitals) }
                            span { "{t(lang, \"spell.skip_capitals\")}" }
                        }
                        label { class: "check",
                            input { r#type: "checkbox", checked: opts.skip_digits,
                                onchange: move |_| sp::set_flag(st, "spell.digits", !opts.skip_digits) }
                            span { "{t(lang, \"spell.skip_digits\")}" }
                        }
                    }
                    if !added.is_empty() {
                        div { class: "field", label { "{t(lang, \"spell.your_words\")}" }
                            div { class: "spell-words",
                                for w in added {
                                    {
                                        let gone = w.clone();
                                        rsx! {
                                            span { key: "{w}", class: "chip spell-word-chip", "{w}"
                                                button { class: "ibtn", title: "{t(lang, \"spell.remove_word\")}",
                                                    onclick: move |_| sp::remove_word(st, &gone), "×" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    p { class: "hint", "{t(lang, \"spell.more\")} " span { class: "mono", "{folder}" } }
                    div { class: "row",
                        button { class: "tbtn spell-folder", onclick: move |_| sp::open_user_dir(st), "{t(lang, \"spell.open_folder\")}" }
                    }
                }
            }
        }
    }
}
