//! Presets of one's own settings, kept by kind: measurements (scale and
//! number format), scanning (scanner, source, colours, resolution),
//! recompressing pictures, and signing (reason, place, contact, time
//! stamp). The Presets window lists those of a kind: used, added from the
//! current settings or the default ones, renamed, cloned, deleted, moved,
//! imported and exported (Edit › Presets, Measures › Manage, Configure Scan
//! Presets, Recompress Images › Edit, Digital Signatures).

use std::collections::BTreeMap;

use dioxus::prelude::*;

use crate::commands::{handlers, scanning};
use crate::i18n::{Lang, t, tf};
use crate::state::{AppState, Dialog, ToastKind};

/// A preset: its name and the settings it holds (setting key → value).
pub type Preset = (String, BTreeMap<String, String>);

/// The kinds of presets: id, the command naming it, its setting keys.
pub const KINDS: &[(&str, &str, &[&str])] = &[
    (
        "measure",
        "measures",
        &[
            "measure.paper",
            "measure.paper_unit",
            "measure.real",
            "measure.real_unit",
            "measure.decimals",
            "measure.decimal_sep",
            "measure.group_sep",
        ],
    ),
    ("scan", "configureScanPresets", &["scan.last"]),
    (
        "recompress",
        "recompressImages",
        &[
            "recompress.preset",
            "recompress.quality",
            "recompress.dpi",
            "recompress.max_px",
        ],
    ),
    (
        "sign",
        "digiSigList",
        &[
            "sign.reason",
            "sign.location",
            "sign.contact",
            "sign.stamp",
            "sign.tsa",
        ],
    ),
];

fn keys(kind: &str) -> &'static [&'static str] {
    KINDS.iter().find(|k| k.0 == kind).map(|k| k.2).unwrap_or(&[])
}

/// The kind shown and the preset chosen in the Presets window.
#[derive(Debug, Clone, PartialEq)]
pub struct Chosen {
    pub kind: String,
    pub index: Option<usize>,
}

pub static CHOSEN: GlobalSignal<Chosen> = Signal::global(|| Chosen {
    kind: "measure".into(),
    index: None,
});

/// The presets of a kind, in their order.
pub fn load(st: AppState, kind: &str) -> Vec<Preset> {
    if kind == "scan" {
        // The scanning window keeps its presets as settings of its own.
        return scanning::presets(st)
            .into_iter()
            .map(|(n, s)| {
                let v = serde_json::to_string(&s).unwrap_or_default();
                (n, BTreeMap::from([("scan.last".to_string(), v)]))
            })
            .collect();
    }
    serde_json::from_str(&st.value(&format!("{kind}.presets"))).unwrap_or_default()
}

pub fn save(st: AppState, kind: &str, all: &[Preset]) {
    if kind == "scan" {
        let scans: Vec<(String, crate::scanner::ScanSettings)> = all
            .iter()
            .map(|(n, m)| {
                let s = m
                    .get("scan.last")
                    .and_then(|v| serde_json::from_str(v).ok())
                    .unwrap_or_default();
                (n.clone(), s)
            })
            .collect();
        st.set_value("scan.presets", serde_json::to_string(&scans).unwrap_or_default());
        return;
    }
    st.set_value(
        &format!("{kind}.presets"),
        serde_json::to_string(all).unwrap_or_default(),
    );
}

/// The settings in use now.
pub fn current(st: AppState, kind: &str) -> BTreeMap<String, String> {
    keys(kind).iter().map(|k| (k.to_string(), st.value(k))).collect()
}

/// Use a preset: its settings become the ones in use (an empty value is the
/// default of that setting).
pub fn apply(st: AppState, kind: &str, values: &BTreeMap<String, String>) {
    for k in keys(kind) {
        st.set_value(k, values.get(*k).cloned().unwrap_or_default());
    }
}

/// A short description of the settings of a preset.
pub fn summary(lang: Lang, kind: &str, values: &BTreeMap<String, String>) -> String {
    let get = |k: &str| values.get(k).map(String::as_str).unwrap_or("");
    let parts: Vec<String> = match kind {
        "measure" => {
            let scale = if get("measure.paper").is_empty() {
                String::new()
            } else {
                format!(
                    "{} {} = {} {}",
                    get("measure.paper"),
                    get("measure.paper_unit"),
                    get("measure.real"),
                    get("measure.real_unit")
                )
            };
            let decimals = if get("measure.decimals").is_empty() {
                String::new()
            } else {
                tf(lang, "presets.decimals", &[("count", get("measure.decimals"))])
            };
            vec![scale, decimals]
        }
        "scan" => {
            let s: crate::scanner::ScanSettings = serde_json::from_str(get("scan.last")).unwrap_or_default();
            vec![
                s.device.clone(),
                s.source.clone(),
                s.mode.clone(),
                if s.dpi > 0 {
                    format!("{} dpi", s.dpi)
                } else {
                    String::new()
                },
            ]
        }
        "recompress" => {
            let preset = get("recompress.preset");
            if preset == "custom" || preset.is_empty() && !get("recompress.quality").is_empty() {
                vec![
                    tf(lang, "presets.quality", &[("value", get("recompress.quality"))]),
                    if get("recompress.dpi").is_empty() {
                        String::new()
                    } else {
                        format!("{} dpi", get("recompress.dpi"))
                    },
                ]
            } else if preset.is_empty() {
                Vec::new()
            } else {
                vec![t(lang, &format!("recompress.preset.{preset}"))]
            }
        }
        _ => vec![
            get("sign.reason").to_string(),
            get("sign.location").to_string(),
            get("sign.contact").to_string(),
            if get("sign.stamp") == "1" {
                t(lang, "presets.time_stamp")
            } else {
                String::new()
            },
        ],
    };
    let parts: Vec<String> = parts.into_iter().filter(|p| !p.trim().is_empty()).collect();
    if parts.is_empty() {
        t(lang, "presets.defaults")
    } else {
        parts.join(" · ")
    }
}

/// Open the Presets window on a kind.
pub fn manage(st: AppState, kind: &str) {
    let mut c = CHOSEN.write();
    if c.kind != kind {
        c.kind = kind.to_string();
        c.index = None;
    }
    drop(c);
    st.set_dialog(Dialog::Presets);
}

/// A name not used yet by a preset of the kind.
fn free_name(all: &[Preset], base: &str) -> String {
    let base = base.trim();
    if !all.iter().any(|(n, _)| n == base) {
        return base.to_string();
    }
    (2..)
        .map(|i| format!("{base} ({i})"))
        .find(|n| !all.iter().any(|(m, _)| m == n))
        .unwrap_or_default()
}

/// Add a preset (the current settings, or the default ones) and choose it.
fn add(st: AppState, defaults: bool) {
    let kind = CHOSEN.peek().kind.clone();
    let mut all = load(st, &kind);
    let values = if defaults {
        keys(&kind)
            .iter()
            .map(|k| (k.to_string(), String::new()))
            .collect()
    } else {
        current(st, &kind)
    };
    let name = free_name(&all, &t(st.lang(), "presets.new_name"));
    all.push((name, values));
    save(st, &kind, &all);
    CHOSEN.write().index = Some(all.len() - 1);
    if *st.dialog.peek() != Dialog::Presets {
        st.set_dialog(Dialog::Presets);
    }
}

/// The preset chosen, with the window open; says so when none is.
fn chosen(st: AppState) -> Option<(String, usize, Vec<Preset>)> {
    let c = CHOSEN.peek().clone();
    let all = load(st, &c.kind);
    match c.index.filter(|i| *i < all.len()) {
        Some(i) => Some((c.kind, i, all)),
        None => {
            if *st.dialog.peek() != Dialog::Presets {
                st.set_dialog(Dialog::Presets);
            }
            st.toast(ToastKind::Info, t(st.lang(), "presets.choose"), "");
            None
        }
    }
}

/// Use the preset chosen (its settings become the ones in use).
pub fn use_chosen(st: AppState) {
    if let Some((kind, i, all)) = chosen(st) {
        apply(st, &kind, &all[i].1);
        st.toast(
            ToastKind::Ok,
            tf(st.lang(), "presets.used", &[("name", &all[i].0)]),
            "",
        );
    }
}

fn clone(st: AppState) {
    if let Some((kind, i, mut all)) = chosen(st) {
        let name = free_name(&all, &all[i].0);
        let values = all[i].1.clone();
        all.insert(i + 1, (name, values));
        save(st, &kind, &all);
        CHOSEN.write().index = Some(i + 1);
    }
}

fn delete(st: AppState) {
    if let Some((kind, i, mut all)) = chosen(st) {
        all.remove(i);
        save(st, &kind, &all);
        CHOSEN.write().index = (!all.is_empty()).then(|| i.min(all.len() - 1));
    }
}

fn shift(st: AppState, up: bool) {
    if let Some((kind, i, mut all)) = chosen(st) {
        let j = if up {
            i.checked_sub(1)
        } else {
            (i + 1 < all.len()).then_some(i + 1)
        };
        if let Some(j) = j {
            all.swap(i, j);
            save(st, &kind, &all);
            CHOSEN.write().index = Some(j);
        }
    }
}

/// Rename a preset; an empty name or one already used is refused.
pub fn rename(st: AppState, index: usize, name: &str) {
    let kind = CHOSEN.peek().kind.clone();
    let mut all = load(st, &kind);
    let name = name.trim();
    if name.is_empty() || all.iter().enumerate().any(|(j, (n, _))| j != index && n == name) {
        st.toast(ToastKind::Info, t(st.lang(), "presets.name_used"), "");
        return;
    }
    if let Some(p) = all.get_mut(index) {
        p.0 = name.to_string();
        save(st, &kind, &all);
    }
}

/// Edit: the name of the preset chosen, ready to type.
fn edit(st: AppState) {
    if let Some((_, i, _)) = chosen(st) {
        let _ = dioxus::document::eval(&format!(
            "setTimeout(() => {{ const i = document.getElementById('preset-name-{i}'); if (i) {{ i.focus(); i.select(); }} }}, 50);"
        ));
    }
}

/// The presets of the kind shown as a file of their own (JSON).
fn export(st: AppState) {
    let kind = CHOSEN.peek().kind.clone();
    let all = load(st, &kind);
    if all.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "presets.none"), "");
        return;
    }
    let text =
        serde_json::to_string_pretty(&serde_json::json!({ "folitecta_presets": kind, "presets": all }))
            .unwrap_or_default();
    crate::files::save_text_file(st, &format!("{kind}-presets.json"), text);
}

/// Presets read from a file of them: added after the others, renamed when
/// their names are taken.
pub fn import_text(st: AppState, text: &str) -> Result<usize, String> {
    let kind = CHOSEN.peek().kind.clone();
    let v: serde_json::Value = serde_json::from_str(text).map_err(|_| t(st.lang(), "presets.not_presets"))?;
    if v.get("folitecta_presets").and_then(|k| k.as_str()) != Some(kind.as_str()) {
        return Err(t(st.lang(), "presets.other_kind"));
    }
    let read: Vec<Preset> = serde_json::from_value(v.get("presets").cloned().unwrap_or_default())
        .map_err(|_| t(st.lang(), "presets.not_presets"))?;
    let mut all = load(st, &kind);
    let allowed = keys(&kind);
    let n = read.len();
    for (name, values) in read {
        let name = free_name(&all, &name);
        let values = values
            .into_iter()
            .filter(|(k, _)| allowed.contains(&k.as_str()))
            .collect();
        all.push((name, values));
    }
    save(st, &kind, &all);
    Ok(n)
}

fn import(st: AppState) {
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file()
            .await
        else {
            return;
        };
        let text = String::from_utf8_lossy(&file.read().await).to_string();
        match import_text(st, &text) {
            Ok(n) => st.toast(
                ToastKind::Ok,
                tf(st.lang(), "presets.imported", &[("count", &n.to_string())]),
                "",
            ),
            Err(e) => st.error(e),
        }
    });
}

/// Digital Signatures › Reset: the signing settings back to their defaults.
fn reset_signing(st: AppState) {
    for k in keys("sign") {
        st.set_value(k, "");
    }
    st.toast(ToastKind::Ok, t(st.lang(), "presets.sign_reset"), "");
}

/// Digital Signatures › Clone, Edit, Delete: on the signing preset chosen.
fn signing(st: AppState, then: fn(AppState)) {
    if CHOSEN.peek().kind != "sign" {
        manage(st, "sign");
    }
    then(st);
}

handlers! {
    "presets.manage" => |st| { let k = CHOSEN.peek().kind.clone(); manage(st, &k) },
    "presets.addNew" => |st| add(st, false),
    "presets.addNew2" => |st| add(st, true),
    "presets.delete" => delete,
    "presetList.add" => |st| add(st, false),
    "presetList.edit" => edit,
    "presetList.clone" => clone,
    "presetList.delete" => delete,
    "presetList.up" => |st| shift(st, true),
    "presetList.down" => |st| shift(st, false),
    "presetList.import" => import,
    "presetList.export" => export,
    "measures.manage" => |st| manage(st, "measure"),
    "configureScanPresets" => |st| manage(st, "scan"),
    "recompressImages.edit" => |st| manage(st, "recompress"),
    "digiSigList.edit" => |st| signing(st, edit),
    "digiSigList.clone" => |st| signing(st, clone),
    "digiSigList.delete" => |st| signing(st, delete),
    "digiSigList.reset" => reset_signing,
    // Measurement Options: the scale and the number format of measures.
    "prefs.measurement" => |st| st.set_dialog(Dialog::Calibrate),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_names_count_up() {
        let all: Vec<Preset> = vec![("A".into(), BTreeMap::new()), ("A (2)".into(), BTreeMap::new())];
        assert_eq!(free_name(&all, "B"), "B");
        assert_eq!(free_name(&all, "A"), "A (3)");
    }

    #[test]
    fn every_kind_has_settings() {
        for (kind, _, k) in KINDS {
            assert!(!k.is_empty(), "{kind}");
        }
    }
}
