//! Revocation of signature certificates: the options of signature validation
//! (online checks, off unless turned on), the dialog that adds validation
//! material to the document, and the revocation of each certificate in the
//! details of a signature.

use dioxus::prelude::*;
use folitecta_core::digsig::{
    CertRevocation, Check, FetchProblem, RevocationKind, RevocationNote, RevocationSource, RevocationState,
    SignatureStatus, TimeBasis,
};

use crate::commands::revocation::{add_material, now_secs, online, set_online};
use crate::commands::signatures::trusted_certs;
use crate::i18n::{Lang, t, tf};
use crate::state::{AppState, Dialog};

/// A time for display (UTC).
fn date(secs: i64) -> String {
    format!("{} UTC", folitecta_core::util::unix_display(secs.max(0) as u64))
}

/// The checkbox of online revocation checks.
#[component]
fn OnlineCheck(label: String) -> Element {
    let st = use_context::<AppState>();
    let on = online(st);
    rsx! {
        label { class: "check online-revocation",
            input { r#type: "checkbox", checked: on, onchange: move |_| set_online(st, !on) }
            span { "{label}" }
        }
    }
}

#[component]
pub fn SignatureOptionsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    rsx! {
        crate::ui::dialogs::Frame { title: t(lang, "sig.options"),
            OnlineCheck { label: t(lang, "sig.online_revocation") }
            div { class: "hint", "{t(lang, \"sig.online_revocation_hint\")}" }
        }
    }
}

#[component]
pub fn ValidationMaterialDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let signed = st.read_active(|t| t.doc.has_signatures()).unwrap_or(false);
    let on = online(st);
    // The servers an online check would ask, worked out on another thread.
    let mut servers: Signal<Option<Vec<String>>> = use_signal(|| None);
    let job = use_hook(|| st.read_active(|t| (t.doc.validation_job(), t.revocation.clone())));
    use_hook(move || {
        if let Some((job, pool)) = job.clone() {
            spawn(async move {
                let urls = tokio::task::spawn_blocking(move || {
                    let mut urls: Vec<String> = Vec::new();
                    for r in job.revocation_requests(&trusted_certs(), &pool, now_secs()) {
                        if !urls.contains(&r.url) {
                            urls.push(r.url);
                        }
                    }
                    urls
                })
                .await
                .unwrap_or_default();
                servers.set(Some(urls));
            });
        }
    });
    if !signed {
        return rsx! {
            crate::ui::dialogs::Frame { title: t(lang, "sig.material_title"),
                div { class: "hint", "{t(lang, \"sig.material_unsigned\")}" }
            }
        };
    }
    rsx! {
        crate::ui::dialogs::Frame { title: t(lang, "sig.material_title"), wide: true, ok_label: Some(t(lang, "sig.material_add")),
            on_ok: move |_| {
                st.set_dialog(Dialog::None);
                add_material(st);
            },
            div { class: "hint", "{t(lang, \"sig.material_intro\")}" }
            OnlineCheck { label: t(lang, "sig.material_online") }
            if on {
                match servers() {
                    None => rsx! { div { class: "hint", "{t(lang, \"sig.material_finding\")}" } },
                    Some(list) if list.is_empty() => rsx! { div { class: "hint", "{t(lang, \"sig.material_no_servers\")}" } },
                    Some(list) => rsx! {
                        div { class: "hint", "{t(lang, \"sig.material_servers\")}" }
                        ul { class: "material-servers",
                            for (i, u) in list.iter().enumerate() { li { key: "{i}", "{u}" } }
                        }
                    },
                }
            } else {
                div { class: "callout", "{t(lang, \"sig.material_offline\")}" }
            }
        }
    }
}

/// The class of a row: passed, a reservation, failed.
fn row_class(e: &CertRevocation) -> &'static str {
    match e.verdict {
        Check::Passed => "ok",
        Check::Failed => "bad",
        _ => "warn",
    }
}

fn reason_text(lang: Lang, reason: Option<u8>) -> String {
    match reason {
        Some(r @ (0..=6 | 8..=10)) => t(lang, &format!("sig.rev_reason_{r}")),
        Some(_) => t(lang, "sig.rev_reason_0"),
        None => t(lang, "sig.rev_reason_none"),
    }
}

fn state_text(lang: Lang, e: &CertRevocation, at: Option<i64>) -> String {
    match e.state {
        RevocationState::Good => t(lang, "sig.rev_good"),
        RevocationState::Unknown => t(lang, "sig.rev_unknown"),
        RevocationState::Revoked { time, reason } => {
            let args = [("date", date(time)), ("reason", reason_text(lang, reason))];
            let args: Vec<(&str, &str)> = args.iter().map(|(k, v)| (*k, v.as_str())).collect();
            if at.is_some_and(|at| time > at) {
                tf(lang, "sig.rev_revoked_later", &args)
            } else {
                tf(lang, "sig.rev_revoked", &args)
            }
        }
    }
}

/// The text of a note, in the language of the interface.
pub fn note_text(lang: Lang, note: &RevocationNote) -> String {
    match note {
        RevocationNote::FetchFailed { url, problem } => {
            let detail = match problem {
                FetchProblem::Transport(d) => d.clone(),
                FetchProblem::Answer(n) => t(lang, &format!("sig.rev_note_{}", n.key())),
                FetchProblem::AnotherCertificate => t(lang, "sig.rev_note_another_certificate"),
                FetchProblem::NotAsked => t(lang, "sig.rev_note_not_asked"),
            };
            tf(
                lang,
                "sig.rev_note_fetch_failed",
                &[("url", url), ("detail", &detail)],
            )
        }
        n => t(lang, &format!("sig.rev_note_{}", n.key())),
    }
}

fn meta_text(lang: Lang, e: &CertRevocation) -> String {
    let mut parts = Vec::new();
    if let Some(kind) = e.kind {
        parts.push(t(
            lang,
            match kind {
                RevocationKind::Ocsp => "sig.rev_kind_ocsp",
                RevocationKind::Crl => "sig.rev_kind_crl",
            },
        ));
    }
    if !e.authority.is_empty() {
        parts.push(e.authority.clone());
    }
    match (e.this_update, e.next_update) {
        (Some(from), Some(to)) => parts.push(tf(
            lang,
            "sig.rev_dates",
            &[("from", &date(from)), ("to", &date(to))],
        )),
        (Some(from), None) => parts.push(tf(lang, "sig.rev_dates_open", &[("from", &date(from))])),
        _ => {}
    }
    parts.join(" · ")
}

/// The revocation of each certificate checked for a signature.
#[component]
pub fn RevocationList(status: SignatureStatus) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    if status.revocation.is_empty() {
        return rsx! {};
    }
    let on = online(st);
    let basis = match status.revocation_basis {
        TimeBasis::TimeStamp => "sig.rev_basis_time_stamp",
        TimeBasis::Claimed => "sig.rev_basis_claimed",
        TimeBasis::Now => "sig.rev_basis_now",
    };
    let at = status.revocation_at;
    let checked = tf(
        lang,
        "sig.rev_checked_at",
        &[
            ("time", &at.map(date).unwrap_or_default()),
            ("basis", &t(lang, basis)),
        ],
    );
    rsx! {
        div { class: "prop-head", "{t(lang, \"sig.revocation\")}" }
        div { class: "hint", "{checked}" }
        div { class: "rev-list",
            for (i, e) in status.revocation.iter().enumerate() {
                {
                    let class = row_class(e);
                    let source = e.source.map(|s| t(lang, match s {
                        RevocationSource::Signature => "sig.rev_source_signature",
                        RevocationSource::TimeStamp => "sig.rev_source_time_stamp",
                        RevocationSource::Dss => "sig.rev_source_dss",
                        RevocationSource::Fetched => "sig.rev_source_fetched",
                    }));
                    let meta = meta_text(lang, e);
                    let mut notes: Vec<String> = e.notes.iter().map(|n| note_text(lang, n)).collect();
                    if !on && e.state == RevocationState::Unknown && !e.addresses.is_empty() {
                        notes.push(t(lang, "sig.rev_note_online_off"));
                    }
                    rsx! {
                        div { class: "rev-row {class}", key: "{i}",
                            span { class: "sig-dot {class}" }
                            span { class: "k", "{e.subject}" }
                            span { class: "v", "{state_text(lang, e, at)}" }
                            if let Some(src) = source { span { class: "chip rev-source", "{src}" } } else { span {} }
                            if !meta.is_empty() { span { class: "rev-meta", "{meta}" } }
                            for (j, n) in notes.iter().enumerate() { span { class: "rev-note", key: "n{j}", "{n}" } }
                        }
                    }
                }
            }
        }
    }
}
