//! Digital signatures: the signatures pane, the signing dialog, signature
//! details, digital IDs and trusted certificates.

use dioxus::prelude::*;
use folitecta_core::digsig::{
    CertInfo, IdentityInfo, KeyKind, SigAppearance, SignOptions, SignPlacement, SignatureStatus,
    parse_certificates,
};

use crate::commands::signatures::{
    add_trusted, create_id, delete_id, import_id, invalidate_status, load_id, open_signed_revision,
    remove_trusted, signature_status, start_sign, stored_ids, trusted_certs,
};
use crate::i18n::{Lang, t, tf};
use crate::icons::Icon;
use crate::state::{AppState, Dialog, ToastKind};

fn certify_name(lang: Lang, p: u8) -> String {
    t(lang, &format!("sig.certify_{p}"))
}

/// Status class and text of a signature.
fn status_of(lang: Lang, s: &SignatureStatus) -> (&'static str, String) {
    if !s.valid {
        ("bad", t(lang, "sig.invalid"))
    } else if !s.covers_document {
        ("warn", t(lang, "sig.valid_changed"))
    } else if s.trusted {
        ("ok", t(lang, "sig.valid_trusted"))
    } else {
        ("warn", t(lang, "sig.valid_unknown"))
    }
}

#[component]
pub fn SignaturesPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(fields) = st.read_active(|t| t.doc.signatures()) else {
        return rsx! {};
    };
    let status = signature_status(st);
    let empty = st
        .read_active(|t| t.doc.empty_signature_fields())
        .unwrap_or_default();
    rsx! {
        div { class: "pane-toolbar icon-toolbar",
            button { class: "tbtn primary", onclick: move |_| crate::commands::dispatch(st, "document.signatures.sign"), Icon { name: "signature" } "{t(lang, \"sig.sign\")}" }
            button { class: "ibtn", title: "{t(lang, \"cmd.document.signatures.validateAll\")}", onclick: move |_| crate::commands::dispatch(st, "document.signatures.validateAll"), Icon { name: "check" } }
            button { class: "ibtn", title: "{t(lang, \"cmd.signs.digitalIDs\")}", onclick: move |_| st.set_dialog(Dialog::DigitalIds), Icon { name: "lock" } }
            button { class: "ibtn", title: "{t(lang, \"sig.trusted\")}", onclick: move |_| st.set_dialog(Dialog::TrustedCerts), Icon { name: "shield" } }
        }
        if fields.is_empty() {
            div { class: "pane-empty", "{t(lang, \"label.none\")}" }
        }
        for s in status.iter().cloned() {
            {
                let (class, text) = status_of(lang, &s);
                let field = s.field.clone();
                let field_rev = s.field.clone();
                let page = s.page;
                let cert_line = s.cert.as_ref().map(|c| c.issuer_common_name.clone()).unwrap_or_default();
                rsx! {
                    div { class: "list-item sig-item {class}", key: "{s.field}",
                        onclick: move |_| { if let Some(p) = page { st.go_to_page(p); } },
                        div { class: "title",
                            span { class: "sig-dot {class}" }
                            span { class: "ellipsis", "{s.signer}" }
                            if s.certify.is_some() { span { class: "chip", "{t(lang, \"sig.certified\")}" } }
                        }
                        div { class: "sig-state {class}", "{text}" }
                        div { class: "meta",
                            span { "{s.time}" }
                            if !s.reason.is_empty() { span { "{s.reason}" } }
                            if !cert_line.is_empty() { span { "{t(lang, \"sig.issuer\")}: {cert_line}" } }
                        }
                        div { class: "row", style: "gap:6px", onclick: move |e| e.stop_propagation(),
                            button { class: "tbtn", onclick: move |_| st.set_dialog(Dialog::SignatureDetails(field.clone())), "{t(lang, \"sig.details\")}" }
                            button { class: "tbtn", title: "{t(lang, \"sig.signed_version_tip\")}", onclick: move |_| open_signed_revision(st, &field_rev), "{t(lang, \"sig.signed_version\")}" }
                        }
                    }
                }
            }
        }
        for name in empty {
            {
                let n = name.clone();
                rsx! {
                    div { class: "list-item sig-item", key: "empty-{name}",
                        div { class: "title", span { class: "sig-dot" } span { class: "ellipsis", "{name}" } span { class: "chip", "{t(lang, \"label.unsigned\")}" } }
                        div { class: "row",
                            button { class: "tbtn", onclick: move |_| st.set_dialog(Dialog::Sign { certify: 0, invisible: false, field: Some(n.clone()) }), Icon { name: "signature" } "{t(lang, \"sig.sign_field\")}" }
                        }
                    }
                }
            }
        }
    }
}

fn cert_rows(lang: Lang, c: &CertInfo) -> Vec<(String, String)> {
    let mut rows = vec![
        (t(lang, "sig.subject"), c.subject.clone()),
        (t(lang, "sig.issuer"), c.issuer.clone()),
        (t(lang, "sig.valid_from"), c.not_before.clone()),
        (t(lang, "sig.valid_to"), c.not_after.clone()),
        (t(lang, "sig.key"), c.key.clone()),
        (t(lang, "sig.serial"), c.serial.clone()),
        ("SHA-256".into(), c.sha256.clone()),
    ];
    if !c.email.is_empty() {
        rows.insert(1, ("E-mail".into(), c.email.clone()));
    }
    rows
}

#[component]
pub fn SignatureDetailsDialog(field: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(s) = signature_status(st).into_iter().find(|s| s.field == field) else {
        return rsx! {};
    };
    let (class, text) = status_of(lang, &s);
    let field_rev = s.field.clone();
    let cert_rows = s.cert.as_ref().map(|c| cert_rows(lang, c)).unwrap_or_default();
    let chain: Vec<String> = s.chain.iter().map(|c| c.common_name.clone()).collect();
    let certify_text = s.certify.map(|p| t(lang, &format!("sig.certify_{p}")));
    let trusted = s.trusted;
    rsx! {
        crate::ui::dialogs::Frame { title: t(lang, "sig.details_title"), wide: true,
            div { class: "sig-state big {class}", "{text}" }
            if !s.problems.is_empty() {
                ul { class: "problems",
                    for (i, p) in s.problems.iter().enumerate() { li { key: "{i}", "{p}" } }
                }
            }
            div { class: "kv-grid",
                span { class: "k", "{t(lang, \"sig.signer\")}" } span { class: "v", "{s.signer}" }
                span { class: "k", "{t(lang, \"sig.time\")}" } span { class: "v", "{s.time}" }
                if !s.reason.is_empty() { span { class: "k", "{t(lang, \"sig.reason\")}" } span { class: "v", "{s.reason}" } }
                if !s.location.is_empty() { span { class: "k", "{t(lang, \"sig.location\")}" } span { class: "v", "{s.location}" } }
                if !s.contact.is_empty() { span { class: "k", "{t(lang, \"sig.contact\")}" } span { class: "v", "{s.contact}" } }
                if let Some(c) = certify_text { span { class: "k", "{t(lang, \"sig.certified\")}" } span { class: "v", "{c}" } }
                span { class: "k", "{t(lang, \"sig.format\")}" } span { class: "v", "{s.sub_filter} · {s.digest_algorithm}" }
                span { class: "k", "{t(lang, \"sig.field\")}" } span { class: "v", "{s.field}" }
            }
            div { class: "prop-head", "{t(lang, \"sig.certificate\")}" }
            div { class: "kv-grid",
                for (i, (k, v)) in cert_rows.into_iter().enumerate() {
                    span { class: "k", key: "k{i}", "{k}" }
                    span { class: "v mono", "{v}" }
                }
                if !chain.is_empty() {
                    span { class: "k", "{t(lang, \"sig.chain\")}" } span { class: "v", "{chain.join(\" → \")}" }
                }
            }
            div { class: "row",
                if !trusted && s.cert.is_some() {
                    button { class: "tbtn primary",
                        onclick: move |_| trust_signer(st, &field),
                        "{t(lang, \"sig.trust\")}"
                    }
                }
                button { class: "tbtn", onclick: move |_| open_signed_revision(st, &field_rev), "{t(lang, \"sig.signed_version\")}" }
            }
        }
    }
}

/// Add the signer's certificate of a signature to the trusted list.
fn trust_signer(st: AppState, field: &str) {
    let certs = st
        .read_active(|t| t.doc.signer_certificates(field))
        .unwrap_or_default();
    match certs.first() {
        Some(c) => match add_trusted(std::slice::from_ref(c)) {
            Ok(_) => {
                invalidate_status(st);
                st.toast(ToastKind::Ok, t(st.lang(), "toast.cert_trusted"), "");
            }
            Err(e) => st.error(e),
        },
        None => st.toast(ToastKind::Warn, t(st.lang(), "sig.no_cert"), ""),
    }
}

#[component]
pub fn SignDialog(certify: u8, invisible: bool, field: Option<String>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let ids = stored_ids();
    let empty_fields = st
        .read_active(|t| t.doc.empty_signature_fields())
        .unwrap_or_default();
    let get = |k: &str| st.ui.read().value(k).unwrap_or("").to_string();
    let mut selected = use_signal(|| {
        let last = get("sign.id");
        if ids.iter().any(|i| i.file.display().to_string() == last) {
            last
        } else {
            ids.first()
                .map(|i| i.file.display().to_string())
                .unwrap_or_default()
        }
    });
    let mut password = use_signal(String::new);
    let mut reason = use_signal(|| get("sign.reason"));
    let mut location = use_signal(|| get("sign.location"));
    let mut contact = use_signal(|| get("sign.contact"));
    let mut level = use_signal(|| certify);
    let initial_place = match (&field, invisible) {
        (Some(f), _) => format!("field:{f}"),
        (None, true) => "invisible".to_string(),
        (None, false) => empty_fields
            .first()
            .map(|f| format!("field:{f}"))
            .unwrap_or_else(|| "draw".into()),
    };
    let mut place = use_signal(|| initial_place);
    let mut show_reason = use_signal(|| true);
    let mut show_location = use_signal(|| true);
    let mut show_labels = use_signal(|| true);
    let mut image: Signal<Option<(String, Vec<u8>)>> = use_signal(|| None);
    let mut error = use_signal(String::new);
    let title = if certify > 0 {
        t(lang, "cmd.document.signatures.certify")
    } else {
        t(lang, "cmd.document.signatures.sign")
    };
    let has_ids = !ids.is_empty();
    rsx! {
        crate::ui::dialogs::Frame { title: title.trim_end_matches('…').to_string(), wide: true, ok_label: Some(t(lang, "sig.sign")),
            on_ok: move |_| {
                let Some(stored) = stored_ids().into_iter().find(|i| i.file.display().to_string() == selected()) else {
                    error.set(t(lang, "sig.choose_id"));
                    return;
                };
                let id = match load_id(&stored, &password()) {
                    Ok(id) => id,
                    Err(e) => { error.set(e); return; }
                };
                st.set_value("sign.id", selected());
                st.set_value("sign.reason", reason());
                st.set_value("sign.location", location());
                st.set_value("sign.contact", contact());
                let placement = match place().as_str() {
                    "invisible" => SignPlacement::Invisible,
                    p if p.starts_with("field:") => SignPlacement::Field(p[6..].to_string()),
                    _ => SignPlacement::New { page: st.current_page(), rect: [0.0; 4] },
                };
                let opts = SignOptions {
                    reason: reason(),
                    location: location(),
                    contact: contact(),
                    certify: level(),
                    placement,
                    appearance: SigAppearance {
                        show_reason: show_reason(),
                        show_location: show_location(),
                        show_labels: show_labels(),
                        image: image().map(|(_, d)| d),
                        ..SigAppearance::default()
                    },
                    labels: [t(lang, "sig.label_by"), t(lang, "sig.time"), t(lang, "sig.reason"), t(lang, "sig.location")],
                    ..SignOptions::default()
                };
                st.set_dialog(Dialog::None);
                start_sign(st, id, opts);
            },
            if !has_ids {
                div { class: "callout", "{t(lang, \"sig.no_ids\")}"
                    div { class: "row", button { class: "tbtn primary", onclick: move |_| st.set_dialog(Dialog::DigitalIds), "{t(lang, \"cmd.signs.digitalIDs\")}" } }
                }
            }
            div { class: "field-grid",
                div { class: "field",
                    label { "{t(lang, \"sig.id\")}" }
                    select { onchange: move |e| selected.set(e.value()),
                        for i in ids.iter() {
                            option { key: "{i.file.display()}", value: "{i.file.display()}", selected: selected() == i.file.display().to_string(), "{i.info.common_name} ({i.info.not_after})" }
                        }
                    }
                }
                div { class: "field",
                    label { "{t(lang, \"sig.password\")}" }
                    input { r#type: "password", value: "{password}", autofocus: has_ids, oninput: move |e| password.set(e.value()) }
                }
                div { class: "field", label { "{t(lang, \"sig.reason\")}" } input { value: "{reason}", oninput: move |e| reason.set(e.value()) } }
                div { class: "field", label { "{t(lang, \"sig.location\")}" } input { value: "{location}", oninput: move |e| location.set(e.value()) } }
                div { class: "field", label { "{t(lang, \"sig.contact\")}" } input { value: "{contact}", oninput: move |e| contact.set(e.value()) } }
                div { class: "field",
                    label { "{t(lang, \"sig.certification\")}" }
                    select { onchange: move |e| level.set(e.value().parse().unwrap_or(0)),
                        for p in 0u8..4 {
                            option { key: "{p}", value: "{p}", selected: level() == p, "{certify_name(lang, p)}" }
                        }
                    }
                }
            }
            div { class: "field",
                label { "{t(lang, \"sig.placement\")}" }
                div { class: "radio-col",
                    label { input { r#type: "radio", name: "sig-place", checked: place() == "draw", onchange: move |_| place.set("draw".into()) } "{t(lang, \"sig.place_draw\")}" }
                    for f in empty_fields.iter().cloned() {
                        {
                            let v = format!("field:{f}");
                            let v2 = v.clone();
                            rsx! { label { key: "{v}", input { r#type: "radio", name: "sig-place", checked: place() == v, onchange: move |_| place.set(v2.clone()) } "{tf(lang, \"sig.place_field\", &[(\"name\", &f)])}" } }
                        }
                    }
                    label { input { r#type: "radio", name: "sig-place", checked: place() == "invisible", onchange: move |_| place.set("invisible".into()) } "{t(lang, \"sig.place_invisible\")}" }
                }
            }
            if place() != "invisible" {
                div { class: "check-row",
                    label { class: "check mini", input { r#type: "checkbox", checked: show_reason(), onchange: move |_| show_reason.set(!show_reason()) } span { "{t(lang, \"sig.show_reason\")}" } }
                    label { class: "check mini", input { r#type: "checkbox", checked: show_location(), onchange: move |_| show_location.set(!show_location()) } span { "{t(lang, \"sig.show_location\")}" } }
                    label { class: "check mini", input { r#type: "checkbox", checked: show_labels(), onchange: move |_| show_labels.set(!show_labels()) } span { "{t(lang, \"sig.show_labels\")}" } }
                }
                div { class: "row",
                    button { class: "tbtn",
                        onclick: move |_| {
                            spawn(async move {
                                if let Some(f) = rfd::AsyncFileDialog::new().add_filter("Images", &["png", "jpg", "jpeg"]).pick_file().await {
                                    image.set(Some((f.file_name(), f.read().await)));
                                }
                            });
                        },
                        Icon { name: "image" } "{t(lang, \"sig.image\")}"
                    }
                    if let Some((name, _)) = image() {
                        span { class: "chip", "{name}" }
                        button { class: "ibtn", onclick: move |_| image.set(None), "×" }
                    }
                }
            }
            if !error().is_empty() {
                div { class: "callout error", "{error}" }
            }
        }
    }
}

#[component]
pub fn DigitalIdsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut refresh = use_signal(|| 0u32);
    let _ = refresh();
    let ids = stored_ids();
    let mut creating = use_signal(|| false);
    let mut name = use_signal(String::new);
    let mut org = use_signal(String::new);
    let mut unit = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut country = use_signal(String::new);
    let mut kind = use_signal(|| "rsa2048".to_string());
    let mut years = use_signal(|| "3".to_string());
    let mut pw1 = use_signal(String::new);
    let mut pw2 = use_signal(String::new);
    let mut message = use_signal(String::new);
    let import = move |_| {
        spawn(async move {
            let Some(f) = rfd::AsyncFileDialog::new()
                .add_filter("PKCS#12", &["pfx", "p12"])
                .pick_file()
                .await
            else {
                return;
            };
            let data = f.read().await;
            let pw = pw1();
            match import_id(&data, &pw) {
                Ok(id) => {
                    message.set(tf(lang, "sig.id_imported", &[("name", &id.info.common_name)]));
                    refresh += 1;
                }
                Err(e) => message.set(format!("{}: {e}", t(lang, "sig.import_failed"))),
            }
        });
    };
    rsx! {
        crate::ui::dialogs::Frame { title: t(lang, "cmd.signs.digitalIDs").trim_end_matches('…').to_string(), wide: true,
            if ids.is_empty() {
                div { class: "hint", "{t(lang, \"sig.no_ids\")}" }
            }
            div { class: "id-list",
                for id in ids.iter().cloned() {
                    {
                        let file = id.file.clone();
                        let id_del = id.clone();
                        rsx! {
                            div { class: "list-item", key: "{id.file.display()}",
                                div { class: "title", Icon { name: "lock" } span { class: "ellipsis", "{id.info.common_name}" } span { class: "chip", "{id.info.key}" } }
                                div { class: "meta",
                                    if !id.info.email.is_empty() { span { "{id.info.email}" } }
                                    if !id.info.organization.is_empty() { span { "{id.info.organization}" } }
                                    span { "{t(lang, \"sig.valid_to\")}: {id.info.not_after}" }
                                }
                                div { class: "row", style: "gap:6px",
                                    button { class: "tbtn",
                                        onclick: move |_| {
                                            let file = file.clone();
                                            spawn(async move {
                                                let name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                                                if let Some(dst) = rfd::AsyncFileDialog::new().set_file_name(&name).add_filter("PKCS#12", &["pfx"]).save_file().await {
                                                    if let Err(e) = std::fs::copy(&file, dst.path()) { st.error(e); }
                                                }
                                            });
                                        },
                                        Icon { name: "export" } "{t(lang, \"sig.export_id\")}"
                                    }
                                    button { class: "tbtn",
                                        onclick: move |_| {
                                            let cer = id.file.with_extension("cer");
                                            spawn(async move {
                                                if let Some(dst) = rfd::AsyncFileDialog::new().set_file_name("certificate.cer").add_filter("Certificate", &["cer"]).save_file().await {
                                                    if let Err(e) = std::fs::copy(&cer, dst.path()) { st.error(e); }
                                                }
                                            });
                                        },
                                        "{t(lang, \"sig.export_cert\")}"
                                    }
                                    button { class: "ibtn danger", title: "{t(lang, \"label.delete\")}", onclick: move |_| { delete_id(&id_del); refresh += 1; }, Icon { name: "delete" } }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "row",
                button { class: "tbtn", onclick: import, Icon { name: "import" } "{t(lang, \"sig.import_id\")}" }
                button { class: if creating() { "tbtn active" } else { "tbtn" }, onclick: move |_| creating.set(!creating()), Icon { name: "insert" } "{t(lang, \"sig.create_id\")}" }
            }
            div { class: "hint", "{t(lang, \"sig.password_hint\")}" }
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"sig.password\")}" } input { r#type: "password", value: "{pw1}", oninput: move |e| pw1.set(e.value()) } }
                if creating() {
                    div { class: "field", label { "{t(lang, \"sig.password_again\")}" } input { r#type: "password", value: "{pw2}", oninput: move |e| pw2.set(e.value()) } }
                }
            }
            if creating() {
                div { class: "field-grid",
                    div { class: "field", label { "{t(lang, \"sig.name\")}" } input { value: "{name}", oninput: move |e| name.set(e.value()) } }
                    div { class: "field", label { "E-mail" } input { value: "{email}", oninput: move |e| email.set(e.value()) } }
                    div { class: "field", label { "{t(lang, \"sig.organization\")}" } input { value: "{org}", oninput: move |e| org.set(e.value()) } }
                    div { class: "field", label { "{t(lang, \"sig.unit\")}" } input { value: "{unit}", oninput: move |e| unit.set(e.value()) } }
                    div { class: "field", label { "{t(lang, \"sig.country\")}" } input { value: "{country}", maxlength: "2", oninput: move |e| country.set(e.value().to_uppercase()) } }
                    div { class: "field", label { "{t(lang, \"sig.key\")}" }
                        select { onchange: move |e| kind.set(e.value()),
                            option { value: "rsa2048", selected: kind() == "rsa2048", "RSA 2048" }
                            option { value: "rsa3072", selected: kind() == "rsa3072", "RSA 3072" }
                            option { value: "p256", selected: kind() == "p256", "ECDSA P-256" }
                        }
                    }
                    div { class: "field", label { "{t(lang, \"sig.years\")}" } input { r#type: "number", min: "1", max: "30", value: "{years}", oninput: move |e| years.set(e.value()) } }
                }
                div { class: "row",
                    button { class: "tbtn primary",
                        onclick: move |_| {
                            if pw1() != pw2() {
                                message.set(t(lang, "sig.passwords_differ"));
                                return;
                            }
                            let info = IdentityInfo { name: name(), organization: org(), unit: unit(), email: email(), country: country() };
                            let k = match kind().as_str() { "rsa3072" => KeyKind::Rsa3072, "p256" => KeyKind::EcP256, _ => KeyKind::Rsa2048 };
                            match create_id(&info, k, years().parse().unwrap_or(3), &pw1()) {
                                Ok(id) => {
                                    message.set(tf(lang, "sig.id_created", &[("name", &id.info.common_name)]));
                                    creating.set(false);
                                    refresh += 1;
                                }
                                Err(e) => message.set(e),
                            }
                        },
                        "{t(lang, \"sig.create\")}"
                    }
                }
            }
            if !message().is_empty() {
                div { class: "callout", "{message}" }
            }
        }
    }
}

#[component]
pub fn TrustedCertsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut refresh = use_signal(|| 0u32);
    let _ = refresh();
    let certs: Vec<CertInfo> = trusted_certs()
        .iter()
        .map(folitecta_core::digsig::cert_info)
        .collect();
    let mut shown: Signal<Option<CertInfo>> = use_signal(|| None);
    let add = move |_| {
        spawn(async move {
            let Some(f) = rfd::AsyncFileDialog::new()
                .add_filter("Certificates", &["cer", "crt", "pem", "der", "p7b", "p7c"])
                .pick_file()
                .await
            else {
                return;
            };
            match parse_certificates(&f.read().await) {
                Ok(c) => match add_trusted(&c) {
                    Ok(n) => {
                        invalidate_status(st);
                        st.toast(
                            ToastKind::Ok,
                            tf(lang, "toast.certs_added", &[("count", &n.to_string())]),
                            "",
                        );
                        refresh += 1;
                    }
                    Err(e) => st.error(e),
                },
                Err(e) => st.error(e),
            }
        });
    };
    rsx! {
        crate::ui::dialogs::Frame { title: t(lang, "sig.trusted"), wide: true,
            div { class: "hint", "{t(lang, \"sig.trusted_hint\")}" }
            if certs.is_empty() {
                div { class: "pane-empty", "{t(lang, \"label.none\")}" }
            }
            for c in certs.iter().cloned() {
                {
                    let c2 = c.clone();
                    let sha1 = c.sha1.clone();
                    rsx! {
                        div { class: "list-item", key: "{c.sha1}",
                            div { class: "title", Icon { name: "shield" } span { class: "ellipsis", "{c.common_name}" } if c.self_signed { span { class: "chip", "{t(lang, \"sig.self_signed\")}" } } }
                            div { class: "meta", span { "{t(lang, \"sig.valid_to\")}: {c.not_after}" } span { "{c.issuer_common_name}" } }
                            div { class: "row", style: "gap:6px",
                                button { class: "tbtn", onclick: move |_| shown.set(Some(c2.clone())), "{t(lang, \"sig.details\")}" }
                                button { class: "ibtn danger", title: "{t(lang, \"label.delete\")}", onclick: move |_| { remove_trusted(&sha1); invalidate_status(st); refresh += 1; }, Icon { name: "delete" } }
                            }
                        }
                    }
                }
            }
            if let Some(c) = shown() {
                div { class: "kv-grid",
                    for (i, (k, v)) in cert_rows(lang, &c).into_iter().enumerate() {
                        span { class: "k", key: "k{i}", "{k}" }
                        span { class: "v mono", "{v}" }
                    }
                }
            }
            div { class: "row", button { class: "tbtn primary", onclick: add, Icon { name: "insert" } "{t(lang, \"sig.add_trusted\")}" } }
        }
    }
}
