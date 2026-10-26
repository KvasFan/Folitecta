//! The scan dialog (File › From Scanner…): the scanner, its source, colour
//! mode and resolution, presets of these; the backs of the sheets scanned
//! as a second stack; clean-up afterwards. Where the system's own dialog
//! scans (Windows), only what Folitecta does with the pages is asked.

use dioxus::prelude::*;
use folitecta_core::registry;

use crate::commands::scanning::{self, SCAN, ScanOptions};
use crate::i18n::{cmd_label, t, tf};
use crate::scanner::{self, Backend, ScanSettings};
use crate::state::AppState;
use crate::ui::dialogs::Frame;

#[component]
pub fn ScanDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let title = registry()
        .get("newDocScanner")
        .map(|c| cmd_label(lang, c).trim_end_matches('…').to_string())
        .unwrap_or_default();
    let last = scanning::last_settings(st);
    let mut device = use_signal(|| last.device.clone());
    let mut source = use_signal(|| last.source.clone());
    let mut mode = use_signal(|| last.mode.clone());
    let mut dpi = use_signal(|| if last.dpi == 0 { 300 } else { last.dpi });
    let mut backs = use_signal(|| st.value("scan.backs") == "1");
    let mut reversed = use_signal(|| st.value("scan.reversed") != "0");
    let mut clean_up = use_signal(|| st.value("scan.clean_up") == "1");
    let mut preset_name = use_signal(String::new);

    let backend = scanner::backend();
    let state = SCAN.read().clone();
    let devices = state.devices.clone();
    // The scanner chosen: the one used last when it is there, else the first.
    let chosen = devices
        .as_ref()
        .and_then(|d| d.iter().find(|x| x.id == device()).or(d.first()))
        .map(|d| d.id.clone())
        .unwrap_or_default();
    let caps = state
        .caps
        .as_ref()
        .filter(|(d, _)| *d == chosen)
        .map(|(_, c)| c.clone());
    // Choices the scanner does not offer fall back to its first one.
    let pick = |want: String, offered: &[String]| {
        if offered.is_empty() || offered.contains(&want) {
            want
        } else {
            offered[0].clone()
        }
    };
    let (sources, modes, resolutions) = caps
        .as_ref()
        .map(|c| (c.sources.clone(), c.modes.clone(), c.resolutions.clone()))
        .unwrap_or_default();
    let settings = ScanSettings {
        device: chosen.clone(),
        source: pick(source(), &sources),
        mode: pick(mode(), &modes),
        dpi: if resolutions.is_empty() || resolutions.contains(&dpi()) {
            dpi()
        } else {
            *resolutions.iter().min_by_key(|r| r.abs_diff(300)).unwrap_or(&300)
        },
    };
    let options = ScanOptions {
        backs: backs(),
        reversed: reversed(),
        clean_up: clean_up(),
    };
    let presets = scanning::presets(st);
    let waiting_backs = !state.fronts.is_empty();
    let fronts = state.fronts.len().to_string();
    let ready = match backend {
        Backend::Devices => !chosen.is_empty() && caps.is_some(),
        Backend::SystemDialog => true,
        Backend::None => false,
    };
    let ok_label = if waiting_backs {
        t(lang, "scan.scan_backs")
    } else {
        t(lang, "scan.scan")
    };
    let go_settings = settings.clone();
    let preset_settings = settings.clone();
    rsx! {
        Frame { title, ok_label, ok_disabled: !ready || state.busy,
            on_ok: move |_| {
                st.set_value("scan.backs", if backs() { "1" } else { "0" });
                st.set_value("scan.reversed", if reversed() { "1" } else { "0" });
                st.set_value("scan.clean_up", if clean_up() { "1" } else { "0" });
                scanning::start(st, go_settings.clone(), options);
            },
            if backend == Backend::None {
                div { class: "callout warn", "{t(lang, \"scan.unavailable\")}" }
            }
            if backend == Backend::SystemDialog {
                p { class: "hint", "{t(lang, \"scan.system_dialog\")}" }
            }
            if backend == Backend::Devices {
                match devices.as_ref() {
                    None => rsx! { p { class: "hint scan-looking", "{t(lang, \"scan.looking\")}" } },
                    Some(list) if list.is_empty() => rsx! { div { class: "callout warn scan-none", "{t(lang, \"scan.none\")}" } },
                    Some(list) => rsx! {
                        div { class: "field-grid",
                            div { class: "field", label { "{t(lang, \"scan.device\")}" }
                                select { class: "scan-device", disabled: waiting_backs,
                                    onchange: move |e| { device.set(e.value()); scanning::look_at(e.value()); },
                                    for d in list.iter() { option { key: "{d.id}", value: "{d.id}", selected: d.id == chosen, "{d.name}" } }
                                }
                            }
                            if !presets.is_empty() {
                                div { class: "field", label { "{t(lang, \"scan.preset\")}" }
                                    select { class: "scan-preset",
                                        onchange: move |e| {
                                            if let Some((_, p)) = scanning::presets(st).into_iter().find(|(n, _)| *n == e.value()) {
                                                if !p.device.is_empty() { device.set(p.device.clone()); scanning::look_at(p.device.clone()); }
                                                source.set(p.source);
                                                mode.set(p.mode);
                                                if p.dpi > 0 { dpi.set(p.dpi); }
                                                preset_name.set(e.value());
                                            }
                                        },
                                        option { value: "", selected: preset_name().is_empty(), "—" }
                                        for (name, _) in presets.iter() { option { key: "{name}", value: "{name}", selected: *name == preset_name(), "{name}" } }
                                    }
                                }
                            }
                        }
                        if caps.is_none() {
                            p { class: "hint scan-looking", "{t(lang, \"scan.asking\")}" }
                        } else {
                            div { class: "field-grid",
                                if !sources.is_empty() {
                                    div { class: "field", label { "{t(lang, \"scan.source\")}" }
                                        select { class: "scan-source", onchange: move |e| source.set(e.value()),
                                            for s in sources.iter() { option { key: "{s}", value: "{s}", selected: *s == settings.source, "{s}" } }
                                        }
                                    }
                                }
                                if !modes.is_empty() {
                                    div { class: "field", label { "{t(lang, \"scan.mode\")}" }
                                        select { class: "scan-mode", onchange: move |e| mode.set(e.value()),
                                            for m in modes.iter() { option { key: "{m}", value: "{m}", selected: *m == settings.mode, "{m}" } }
                                        }
                                    }
                                }
                                if !resolutions.is_empty() {
                                    div { class: "field", label { "{t(lang, \"scan.resolution\")}" }
                                        select { class: "scan-dpi", onchange: move |e| dpi.set(e.value().parse().unwrap_or(300)),
                                            for r in resolutions.iter() { option { key: "{r}", value: "{r}", selected: *r == settings.dpi, "{r} dpi" } }
                                        }
                                    }
                                }
                            }
                            div { class: "scan-preset-save",
                                input { class: "scan-preset-name", placeholder: "{t(lang, \"scan.preset_name\")}", value: "{preset_name}",
                                    oninput: move |e| preset_name.set(e.value()) }
                                button { class: "tbtn", disabled: preset_name().trim().is_empty(),
                                    onclick: move |_| scanning::save_preset(st, &preset_name(), &preset_settings),
                                    "{t(lang, \"scan.save_preset\")}" }
                                if presets.iter().any(|(n, _)| *n == preset_name()) {
                                    button { class: "tbtn", onclick: move |_| { scanning::delete_preset(st, &preset_name()); preset_name.set(String::new()); },
                                        "{t(lang, \"scan.delete_preset\")}" }
                                }
                            }
                        }
                    },
                }
            }
            if backend != Backend::None {
                label { class: "check", input { class: "scan-backs", r#type: "checkbox", checked: backs(), disabled: waiting_backs, onchange: move |_| backs.set(!backs()) } span { "{t(lang, \"scan.backs\")}" } }
                if backs() {
                    label { class: "check sub", input { r#type: "checkbox", checked: reversed(), onchange: move |_| reversed.set(!reversed()) } span { "{t(lang, \"scan.reversed\")}" } }
                }
                label { class: "check", input { class: "scan-clean", r#type: "checkbox", checked: clean_up(), onchange: move |_| clean_up.set(!clean_up()) } span { "{t(lang, \"scan.clean_up\")}" } }
            }
            if waiting_backs {
                div { class: "callout scan-turn",
                    span { "{tf(lang, \"scan.turn_over\", &[(\"count\", &fronts)])}" }
                    button { class: "tbtn scan-no-backs", disabled: state.busy, onclick: move |_| scanning::without_backs(st, options), "{t(lang, \"scan.no_backs\")}" }
                }
            }
            if state.busy {
                p { class: "hint scan-busy", "{t(lang, \"scan.scanning\")}" }
            }
            if let Some(why) = state.problem.as_ref() {
                div { class: "callout warn scan-problem", "{why}" }
            }
        }
    }
}
