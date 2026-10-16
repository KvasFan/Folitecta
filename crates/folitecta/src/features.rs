//! Generates `docs/FEATURES.md` from the command registry and the handler table.

use std::collections::HashSet;

use folitecta_core::commands::{MENUS, RIBBON_GROUPS, RIBBON_TABS, Registry};
use folitecta_core::registry;

use crate::commands::{implemented_ids, partial_ids};

fn status_of(id: &str, done: &HashSet<&str>, partial: &HashSet<&str>) -> &'static str {
    if done.contains(id) {
        "✅ available"
    } else if partial.contains(id) {
        "🟡 partial"
    } else {
        "⬜ planned"
    }
}

pub fn feature_matrix_markdown() -> String {
    let r: &Registry = registry();
    let done = implemented_ids();
    let partial = partial_ids();
    let (n_done, n_partial, n_planned) = r.status_summary(&done, &partial);
    let mut md = String::new();
    md.push_str("# Folitecta feature matrix\n\n");
    md.push_str(&format!(
        "Generated from the command registry for version {}. Every command of the full feature set is \
         present in the user interface; this table records which ones perform their function today.\n\n",
        folitecta_core::VERSION
    ));
    md.push_str("| Status | Count |\n|---|---:|\n");
    md.push_str(&format!("| ✅ available | {n_done} |\n| 🟡 partial | {n_partial} |\n| ⬜ planned | {n_planned} |\n| total | {} |\n\n", r.len()));
    md.push_str("Legend: *available* performs the complete operation; *partial* performs a reduced version \
                 (documented in the UI); *planned* is listed in menus and ribbon but shows a roadmap notice when invoked.\n\n");

    md.push_str("## Ribbon\n\n");
    for (tab, tab_label) in RIBBON_TABS {
        md.push_str(&format!("### {tab_label}\n\n"));
        for (t, g, g_label) in RIBBON_GROUPS.iter().filter(|(t, _, _)| t == tab) {
            let cmds = r.ribbon_group(t, g);
            if cmds.is_empty() {
                continue;
            }
            md.push_str(&format!(
                "**{g_label}**\n\n| Command | Id | Status |\n|---|---|---|\n"
            ));
            for c in cmds {
                md.push_str(&format!(
                    "| {} | `{}` | {} |\n",
                    c.label,
                    c.id,
                    status_of(&c.id, &done, &partial)
                ));
                for child in r.children(&c.id) {
                    md.push_str(&format!(
                        "| ↳ {} | `{}` | {} |\n",
                        child.label,
                        child.id,
                        status_of(&child.id, &done, &partial)
                    ));
                }
            }
            md.push('\n');
        }
    }

    md.push_str("## Classic menus\n\n");
    for (menu, menu_label) in MENUS {
        md.push_str(&format!(
            "### {menu_label}\n\n| Command | Id | Status |\n|---|---|---|\n"
        ));
        for entry in r.menu_entries(menu).into_iter().flatten() {
            md.push_str(&format!(
                "| {} | `{}` | {} |\n",
                entry.label,
                entry.id,
                status_of(&entry.id, &done, &partial)
            ));
        }
        md.push('\n');
    }

    md.push_str("## Everything else (panes, dialogs, contextual tools)\n\n| Command | Path | Status |\n|---|---|---|\n");
    let in_menus: HashSet<&str> = MENUS
        .iter()
        .flat_map(|(m, _)| r.menu_entries(m).into_iter().flatten().map(|c| c.id.as_str()))
        .collect();
    let in_ribbon: HashSet<&str> = RIBBON_GROUPS
        .iter()
        .flat_map(|(t, g, _)| {
            r.ribbon_group(t, g).into_iter().flat_map(|c| {
                std::iter::once(c.id.as_str()).chain(r.children(&c.id).into_iter().map(|k| k.id.as_str()))
            })
        })
        .collect();
    for c in r.all() {
        if in_menus.contains(c.id.as_str()) || in_ribbon.contains(c.id.as_str()) {
            continue;
        }
        md.push_str(&format!(
            "| `{}` | {} | {} |\n",
            c.id,
            r.path_label(&c.id),
            status_of(&c.id, &done, &partial)
        ));
    }
    md
}

/// Write the matrix to `path` and return a short statistics line.
pub fn write_feature_matrix(path: &str) -> Result<String, String> {
    let md = feature_matrix_markdown();
    std::fs::write(path, &md).map_err(|e| e.to_string())?;
    let r = registry();
    let (d, p, n) = r.status_summary(&implemented_ids(), &partial_ids());
    Ok(format!(
        "{} commands, {d} available, {p} partial, {n} planned",
        r.len()
    ))
}
