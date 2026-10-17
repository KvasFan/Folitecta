//! Accessibility: the tags pane, the reading order and content panes,
//! automatic tagging, alternate text and the accessibility check and report.

use dioxus::prelude::*;
use folitecta_core::accessibility::{CheckResult, CheckStatus};
use folitecta_core::structure::{OrderItem, StructElem, StructKid};

use super::{confirm, handlers, no_doc, option_on, show_pane, toggle_pane};
use crate::i18n::{t, tf};
use crate::state::{AppState, CheckState, ConfirmAction, Dialog, Pane, ToastKind, Verdict};

pub type Id = (u32, u16);

/// Tags of the reading order commands and the standard types they set.
pub const ORDER_TAGS: &[(&str, &str)] = &[
    ("orderview.tag.h1", "H1"),
    ("orderview.tag.h2", "H2"),
    ("orderview.tag.h3", "H3"),
    ("orderview.tag.h4", "H4"),
    ("orderview.tag.h5", "H5"),
    ("orderview.tag.h6", "H6"),
    ("orderview.tag.p", "P"),
    ("orderview.tag.figure", "Figure"),
    ("orderview.tag.caption", "Caption"),
    ("orderview.tag.table", "Table"),
    ("orderview.tag.cell", "TD"),
    ("orderview.tag.listItem", "LI"),
    ("orderview.tag.field", "Form"),
    ("orderview.tag.formula", "Formula"),
    ("orderview.tag.note", "Note"),
    ("orderview.tag.ref", "Reference"),
    ("orderview.tag.artifact", "Artifact"),
];

// ---- Tags -----------------------------------------------------------------------

pub fn structure(st: AppState) -> Option<StructElem> {
    st.read_active(|t| t.doc.structure()).flatten()
}

fn need_tags(st: AppState) -> Option<StructElem> {
    if no_doc(st) {
        return None;
    }
    let tree = structure(st);
    if tree.is_none() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_tags"), "");
    }
    tree
}

pub fn selected_tag(st: AppState) -> Option<Id> {
    st.read_active(|t| t.acc.tag).flatten()
}

fn need_selected(st: AppState) -> Option<(StructElem, Id)> {
    let tree = need_tags(st)?;
    match selected_tag(st).filter(|id| tree.find(*id).is_some()) {
        Some(id) => Some((tree, id)),
        None => {
            st.toast(ToastKind::Info, t(st.lang(), "toast.select_tag"), "");
            None
        }
    }
}

/// Select an element of the tags pane (its content is highlighted on the page).
pub fn select_tag(st: AppState, id: Option<Id>) {
    st.with_active(|t| {
        t.acc.tag = id;
        // The elements around it open.
        if let (Some(id), Some(tree)) = (id, t.doc.structure()) {
            let mut cur = id;
            while let Some(p) = tree.parent_of(cur) {
                t.acc.collapsed.remove(&p.id);
                cur = p.id;
            }
        }
    });
    if let Some(id) = id {
        if option_on(st, "tagsview.highlightContent") {
            show_tag_content(st, id, false);
        }
    }
}

/// Show an element's content on its page.
pub fn show_tag_content(st: AppState, id: Id, announce: bool) {
    let target = st
        .read_active(|t| {
            let tree = t.doc.structure()?;
            let e = tree.find(id)?;
            let p = e.first_page()?;
            Some((p, t.doc.struct_rect(e, p)))
        })
        .flatten();
    match target {
        Some((p, rect)) => {
            if st.current_page() != p {
                st.navigate(p, true);
            }
            st.with_active(|t| t.flash = rect.map(|r| (p, r)));
        }
        None if announce => st.toast(ToastKind::Info, t(st.lang(), "toast.tag_no_content"), ""),
        None => {}
    }
}

fn result_toast(st: AppState, key: &str, n: usize) {
    st.toast(
        ToastKind::Ok,
        tf(st.lang(), key, &[("count", &n.to_string())]),
        "",
    );
}

fn count_op(
    st: AppState,
    key: &str,
    f: impl FnOnce(&mut crate::state::DocTab) -> folitecta_core::Result<usize>,
) {
    if no_doc(st) {
        return;
    }
    match st.with_active(f) {
        Some(Ok(n)) => {
            crate::files::set_window_title(st);
            result_toast(st, key, n);
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn create_root(st: AppState) {
    if no_doc(st) {
        return;
    }
    let r = st.with_active(|t| t.doc.create_struct_root());
    if let Some(Err(e)) = r {
        st.toast(ToastKind::Info, e.to_string(), "");
    } else {
        crate::files::set_window_title(st);
        show_pane(st, Pane::Tags);
    }
}

fn change_to_artifact(st: AppState) {
    let Some((_, id)) = need_selected(st) else { return };
    let r = st.with_active(|t| {
        t.acc.tag = None;
        t.flash = None;
        t.doc.struct_to_artifact(id)
    });
    super::core_result(st, r);
}

fn delete_tag(st: AppState) {
    let Some((_, id)) = need_selected(st) else { return };
    let r = st.with_active(|t| {
        t.acc.tag = None;
        t.flash = None;
        t.doc.delete_struct_elem(id)
    });
    super::core_result(st, r);
}

fn copy_contents(st: AppState) {
    let Some((tree, id)) = need_selected(st) else {
        return;
    };
    let text = tree
        .find(id)
        .and_then(|e| st.read_active(|t| t.doc.struct_text(e)))
        .unwrap_or_default();
    if text.trim().is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.tag_no_text"), "");
    } else {
        crate::clipboard::set_text(&text);
        st.toast(ToastKind::Ok, t(st.lang(), "toast.copied"), "");
    }
}

/// The area of the text selection (display space), on its page.
fn selection_area(st: AppState) -> Option<(usize, [f32; 4])> {
    st.read_active(|t| {
        let s = t.text_sel.filter(|s| !s.is_empty())?;
        let (a, b) = s.range();
        let rects = t.doc.text_layers().get(s.page)?.range_rects(a, b);
        let mut r = *rects.first()?;
        for x in &rects[1..] {
            r = [r[0].min(x[0]), r[1].min(x[1]), r[2].max(x[2]), r[3].max(x[3])];
        }
        Some((s.page, r))
    })
    .flatten()
}

fn tag_from_selection(st: AppState) {
    if no_doc(st) {
        return;
    }
    if let Some((page, rect)) = selection_area(st) {
        st.set_dialog(Dialog::TagFromArea { page, rect });
        return;
    }
    if let Some((page, index)) = st.read_active(|t| t.selected_annot).flatten() {
        let parent = selected_tag(st);
        let r = st.with_active(|t| t.doc.tag_annotation(page, index, parent));
        match r {
            Some(Ok(id)) => {
                crate::files::set_window_title(st);
                show_pane(st, Pane::Tags);
                select_tag(st, Some(id));
            }
            Some(Err(e)) => st.error(e),
            None => {}
        }
        return;
    }
    st.toast(ToastKind::Info, t(st.lang(), "toast.select_content_first"), "");
}

fn find_from_selection(st: AppState) {
    if need_tags(st).is_none() {
        return;
    }
    let found = if let Some((page, rect)) = selection_area(st) {
        st.read_active(|t| t.doc.struct_at(page, rect)).flatten()
    } else if let Some((page, index)) = st.read_active(|t| t.selected_annot).flatten() {
        st.read_active(|t| t.doc.struct_for_annotation(page, index))
            .flatten()
    } else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_content_first"), "");
        return;
    };
    match found {
        Some(id) => {
            show_pane(st, Pane::Tags);
            select_tag(st, Some(id));
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.no_tag_found"), ""),
    }
}

fn is_tagged(st: AppState) {
    if no_doc(st) {
        return;
    }
    let r = st.with_active(|t| {
        let on = !t.doc.is_tagged();
        t.doc.set_tagged(on)
    });
    super::core_result(st, r);
}

fn new_tag(st: AppState, child: bool) {
    if no_doc(st) {
        return;
    }
    let sel = selected_tag(st);
    if child && sel.is_none() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_tag"), "");
        return;
    }
    st.set_dialog(Dialog::TagNew {
        parent: sel,
        index: None,
    });
}

fn new_sibling(st: AppState) {
    let Some((tree, id)) = need_selected(st) else {
        return;
    };
    let Some(parent) = tree.parent_of(id) else { return };
    let index = parent
        .kids
        .iter()
        .position(|k| matches!(k, StructKid::Elem(e) if e.id == id))
        .map(|i| i + 1);
    st.set_dialog(Dialog::TagNew {
        parent: (!parent.is_root()).then_some(parent.id),
        index,
    });
}

fn show_in_content(st: AppState) {
    let Some((tree, id)) = need_selected(st) else {
        return;
    };
    let first = tree.find(id).and_then(|e| e.content().first().copied());
    match first {
        Some((page, mcid)) => {
            st.with_active(|t| t.acc.content = Some((page, Some(mcid))));
            show_pane(st, Pane::Content);
            if st.current_page() != page {
                st.navigate(page, true);
            }
            show_content_item(st, page, Some(mcid));
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.tag_no_content"), ""),
    }
}

/// Move the selected tag up or down among its siblings.
pub fn move_tag(st: AppState, delta: isize) {
    let Some((tree, id)) = need_selected(st) else {
        return;
    };
    let Some(parent) = tree.parent_of(id) else { return };
    let Some(pos) = parent
        .kids
        .iter()
        .position(|k| matches!(k, StructKid::Elem(e) if e.id == id))
    else {
        return;
    };
    // The next element sibling in that direction.
    let mut j = pos as isize + delta;
    while j >= 0 && (j as usize) < parent.kids.len() && !matches!(parent.kids[j as usize], StructKid::Elem(_))
    {
        j += delta;
    }
    if j < 0 || j as usize >= parent.kids.len() {
        return;
    }
    let to = if delta > 0 { j as usize + 1 } else { j as usize };
    let parent_id = (!parent.is_root()).then_some(parent.id);
    let r = st.with_active(|t| t.doc.move_struct_elem(id, parent_id, to));
    super::core_result(st, r);
}

/// Move the selected tag into the element before it, or out of its parent.
pub fn indent_tag(st: AppState, inward: bool) {
    let Some((tree, id)) = need_selected(st) else {
        return;
    };
    let Some(parent) = tree.parent_of(id) else { return };
    let r = if inward {
        let pos = parent
            .kids
            .iter()
            .position(|k| matches!(k, StructKid::Elem(e) if e.id == id))
            .unwrap_or(0);
        let prev = parent.kids[..pos].iter().rev().find_map(|k| match k {
            StructKid::Elem(e) => Some((e.id, e.kids.len())),
            _ => None,
        });
        let Some((target, n)) = prev else { return };
        st.with_active(|t| t.doc.move_struct_elem(id, Some(target), n))
    } else {
        if parent.is_root() {
            return;
        }
        let Some(grand) = tree.parent_of(parent.id) else {
            return;
        };
        let pos = grand
            .kids
            .iter()
            .position(|k| matches!(k, StructKid::Elem(e) if e.id == parent.id))
            .unwrap_or(0);
        let g = (!grand.is_root()).then_some(grand.id);
        st.with_active(|t| t.doc.move_struct_elem(id, g, pos + 1))
    };
    super::core_result(st, r);
}

// ---- Auto-tag ------------------------------------------------------------------

fn auto_tag(st: AppState) {
    if no_doc(st) {
        return;
    }
    if st.read_active(|t| t.doc.has_structure()).unwrap_or(false) {
        confirm(
            st,
            "acc.autotag.replace_title",
            "acc.autotag.replace_body",
            ConfirmAction::AutoTag,
        );
    } else {
        run_auto_tag(st);
    }
}

pub fn run_auto_tag(st: AppState) {
    let r = st.with_active(|t| {
        t.acc.tag = None;
        t.flash = None;
        t.doc.auto_tag()
    });
    match r {
        Some(Ok(rep)) => {
            crate::files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(
                    st.lang(),
                    "toast.autotagged",
                    &[
                        ("headings", &rep.headings.to_string()),
                        ("paragraphs", &rep.paragraphs.to_string()),
                        ("figures", &rep.figures.to_string()),
                        ("annotations", &rep.annotations.to_string()),
                    ],
                ),
                "",
            );
            show_pane(st, Pane::Tags);
            if has_check(st) {
                run_check(st);
            }
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn alt_text(st: AppState) {
    if need_tags(st).is_none() {
        return;
    }
    let n = st
        .read_active(|t| t.doc.struct_elems_of("Figure").len())
        .unwrap_or(0);
    if n == 0 {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_figures"), "");
    } else {
        st.set_dialog(Dialog::AltText);
    }
}

// ---- Reading order -------------------------------------------------------------

/// Items of the reading order of the current page, as the order pane shows them.
pub fn order_items(st: AppState) -> Vec<OrderItem> {
    let cells = st.value("order.cells") == "1";
    let only = st.value("order.tf") == "1";
    let page = st.current_page();
    st.read_active(|t| t.doc.reading_order(page, cells))
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !only || matches!(i.role.as_str(), "Table" | "Figure" | "TD" | "TH"))
        .collect()
}

pub fn order_mode(st: AppState) -> String {
    match st.value("order.mode").as_str() {
        "" => "order".into(),
        m => m.into(),
    }
}

/// Select an item of the reading order and show it on the page.
pub fn select_order(st: AppState, i: Option<usize>) {
    st.with_active(|t| t.acc.order = i);
    if let Some(item) = i.and_then(|i| order_items(st).into_iter().nth(i)) {
        let page = st.current_page();
        st.with_active(|t| t.flash = Some((page, item.rect)));
    }
}

fn selected_order(st: AppState) -> Option<OrderItem> {
    if no_doc(st) {
        return None;
    }
    let i = st.read_active(|t| t.acc.order).flatten();
    match i.and_then(|i| order_items(st).into_iter().nth(i)) {
        Some(item) => Some(item),
        None => {
            st.toast(ToastKind::Info, t(st.lang(), "toast.select_order_item"), "");
            None
        }
    }
}

fn order_tag(st: AppState, tag: &str) {
    let Some(item) = selected_order(st) else { return };
    let r = if tag == "Artifact" {
        st.with_active(|t| {
            t.acc.order = None;
            t.flash = None;
            t.doc.struct_to_artifact(item.id)
        })
    } else {
        st.with_active(|t| t.doc.set_struct_tag(item.id, tag))
    };
    super::core_result(st, r);
}

/// Move an item of the reading order of the current page.
pub fn move_order(st: AppState, from: usize, to: usize) {
    let cells = st.value("order.cells") == "1";
    let page = st.current_page();
    let r = st.with_active(|t| t.doc.move_reading_order(page, from, to, cells));
    if let Some(Ok(())) = r {
        st.with_active(|t| t.acc.order = Some(to));
    }
    super::core_result(st, r);
}

fn group_similar(st: AppState) {
    let cells = st.value("order.cells") == "1";
    let page = st.current_page();
    count_op(st, "toast.grouped", |t| t.doc.group_similar(page, cells));
}

fn flip_value(st: AppState, key: &str) {
    let on = st.value(key) == "1";
    st.set_value(key, if on { "" } else { "1" });
}

// ---- Content ---------------------------------------------------------------------

/// Show an entry of the content pane on its page.
pub fn show_content_item(st: AppState, page: usize, mcid: Option<i64>) {
    st.with_active(|t| t.acc.content = Some((page, mcid)));
    if !option_on(st, "contentsView.displaySelected") {
        return;
    }
    let rect = st
        .read_active(|t| {
            t.doc
                .marked_content(page)
                .iter()
                .find(|m| m.mcid == mcid && (mcid.is_some() || !m.artifact))
                .map(|m| m.rect)
        })
        .flatten();
    if let Some(r) = rect {
        st.with_active(|t| t.flash = Some((page, r)));
    }
}

fn content_in_tags(st: AppState) {
    if no_doc(st) {
        return;
    }
    let found = st
        .read_active(|t| {
            let (page, mcid) = t.acc.content?;
            t.doc.struct_for_content(page, mcid?)
        })
        .flatten();
    match found {
        Some(id) => {
            show_pane(st, Pane::Tags);
            select_tag(st, Some(id));
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.no_tag_found"), ""),
    }
}

// ---- Accessibility check ----------------------------------------------------------

fn has_check(st: AppState) -> bool {
    st.read_active(|t| t.acc.check.is_some()).unwrap_or(false)
}

/// Categories left out of the check by the options.
pub fn categories_off(st: AppState) -> Vec<String> {
    st.value("acc.off")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// Run the accessibility check of the active document.
pub fn run_check(st: AppState) {
    if no_doc(st) {
        return;
    }
    let when = folitecta_core::util::unix_display(crate::state::now_secs());
    st.with_active(|t| {
        let results = t.doc.accessibility_check();
        let prev = t.acc.check.take().unwrap_or_default();
        t.acc.check = Some(CheckState {
            results,
            verdicts: prev.verdicts,
            rule: prev.rule,
            item: None,
            expanded: prev.expanded,
            collapsed: prev.collapsed,
            when,
        });
    });
}

/// Result of a rule with the decision of a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    Passed,
    Failed,
    Manual,
    Skipped,
}

pub fn shown_status(cs: &CheckState, r: &CheckResult) -> Shown {
    match cs.verdicts.get(r.rule) {
        Some(Verdict::Passed) => Shown::Passed,
        Some(Verdict::Failed) => Shown::Failed,
        Some(Verdict::Skipped) => Shown::Skipped,
        None => match r.status {
            CheckStatus::Passed => Shown::Passed,
            CheckStatus::Failed => Shown::Failed,
            CheckStatus::Manual => Shown::Manual,
        },
    }
}

fn check_then(st: AppState, pane: Pane) {
    run_check(st);
    if st.has_doc() {
        show_pane(st, pane);
    }
}

fn selected_rule(st: AppState) -> Option<String> {
    if no_doc(st) {
        return None;
    }
    let rule = st
        .read_active(|t| t.acc.check.as_ref().and_then(|c| c.rule.clone()))
        .flatten();
    if rule.is_none() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_rule"), "");
    }
    rule
}

fn verdict(st: AppState, v: Verdict) {
    let Some(rule) = selected_rule(st) else { return };
    st.with_active(|t| {
        if let Some(c) = t.acc.check.as_mut() {
            if c.verdicts.get(&rule) == Some(&v) {
                c.verdicts.remove(&rule);
            } else {
                c.verdicts.insert(rule.clone(), v);
            }
        }
    });
}

/// Fix a rule of the check (when it can be fixed) and check again.
pub fn fix_rule(st: AppState, rule: &str) {
    if no_doc(st) {
        return;
    }
    let name = st
        .read_active(|t| {
            let n = t.doc.display_name();
            std::path::Path::new(&n)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or(n)
        })
        .unwrap_or_default();
    let counted =
        |st: AppState, key: &str, f: fn(&mut folitecta_core::Document) -> folitecta_core::Result<usize>| {
            count_op(st, key, |t| f(&mut t.doc));
            run_check(st);
        };
    match rule {
        "tagged" => auto_tag(st),
        "language" => st.set_dialog(Dialog::DocLanguage),
        "figures_alt" | "other_alt" => alt_text(st),
        "title" => {
            let r = st.with_active(|t| t.doc.fix_title(&name));
            super::core_result(st, r);
            run_check(st);
        }
        "bookmarks" => counted(st, "toast.bookmarks_made", |d| d.bookmarks_from_structure()),
        "tagged_annotations" => counted(st, "toast.annotations_tagged", |d| d.tag_annotations()),
        "tagged_fields" => counted(st, "toast.fields_tagged", |d| d.tag_form_fields()),
        "tab_order" => counted(st, "toast.tab_order_fixed", |d| d.set_tab_order_structure()),
        "field_descriptions" => counted(st, "toast.descriptions_made", |d| d.fill_field_descriptions()),
        _ => st.toast(ToastKind::Info, t(st.lang(), "toast.no_fix"), ""),
    }
}

fn fix_selected(st: AppState) {
    if let Some(rule) = selected_rule(st) {
        fix_rule(st, &rule);
    }
}

fn explain(st: AppState) {
    if let Some(rule) = selected_rule(st) {
        st.set_dialog(Dialog::RuleExplain(rule));
    }
}

/// The selected item of the check.
fn selected_item(st: AppState) -> Option<folitecta_core::accessibility::CheckItem> {
    let item = st
        .read_active(|t| {
            let c = t.acc.check.as_ref()?;
            let rule = c.rule.as_ref()?;
            let r = c.results.iter().find(|r| r.rule == rule)?;
            r.items.get(c.item?).cloned()
        })
        .flatten();
    if item.is_none() && st.has_doc() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_check_item"), "");
    }
    item
}

/// Show an item of the check on its page.
pub fn show_check_item(st: AppState, item: &folitecta_core::accessibility::CheckItem) {
    if let Some(p) = item.page {
        if st.current_page() != p {
            st.navigate(p, true);
        }
        if let Some(r) = item.rect {
            st.with_active(|t| t.flash = Some((p, r)));
        }
    }
}

fn show_in(st: AppState, pane: Pane) {
    let Some(item) = selected_item(st) else { return };
    match pane {
        Pane::Tags => match item.elem {
            Some(id) => {
                show_pane(st, Pane::Tags);
                select_tag(st, Some(id));
            }
            None => st.toast(ToastKind::Info, t(st.lang(), "toast.no_tag_found"), ""),
        },
        Pane::Content => match item.page {
            Some(p) => {
                show_pane(st, Pane::Content);
                show_check_item(st, &item);
                st.with_active(|t| t.acc.content = Some((p, None)));
            }
            None => st.toast(ToastKind::Info, t(st.lang(), "toast.no_page"), ""),
        },
        _ => match item.annot {
            Some(a) => {
                st.with_active(|t| t.selected_annot = Some(a));
                show_pane(st, pane);
                show_check_item(st, &item);
            }
            None => st.toast(ToastKind::Info, t(st.lang(), "toast.no_annotation_item"), ""),
        },
    }
}

/// Check states of the accessibility commands.
pub fn acc_state(st: AppState, id: &str) -> Option<bool> {
    let pane = |p: &str| {
        let ui = st.ui.peek();
        ui.show_left_pane && ui.left_pane == p
    };
    match id {
        "view.tags" => Some(pane("tags")),
        "view.order" => Some(pane("order")),
        "view.content" => Some(pane("content")),
        "view.accChecker" => Some(pane("accessibility-checker")),
        "view.accReport" => Some(pane("accessibility-report")),
        "tagsview.isTagged" => Some(st.read_active(|t| t.doc.is_tagged()).unwrap_or(false)),
        "orderview.show.tableCells" => Some(st.value("order.cells") == "1"),
        "orderview.show.tablesAndFigures" => Some(st.value("order.tf") == "1"),
        "orderview.showmode.none" | "orderview.showmode.order" | "orderview.showmode.type" => {
            Some(id.ends_with(&order_mode(st)))
        }
        _ => None,
    }
}

fn set_mode(st: AppState, mode: &str) {
    st.set_value("order.mode", if mode == "order" { "" } else { mode });
}

handlers! {
    "accessibility" => |st| check_then(st, Pane::AccChecker),
    "tags" => |st| show_pane(st, Pane::Tags),
    "view.tags" => |st| toggle_pane(st, Pane::Tags),
    "view.order" => |st| toggle_pane(st, Pane::Order),
    "view.content" => |st| toggle_pane(st, Pane::Content),
    "view.accChecker" => |st| toggle_pane(st, Pane::AccChecker),
    "view.accReport" => |st| toggle_pane(st, Pane::AccReport),
    "acc.alt" => alt_text,
    "acc.autotag.document" => auto_tag,
    "acc.autotag.form" => |st| count_op(st, "toast.fields_tagged", |t| t.doc.tag_form_fields()),
    "acc.check.document" => |st| check_then(st, Pane::AccChecker),
    "acc.check.report" => |st| check_then(st, Pane::AccReport),
    "accCheckerView.options" => |st| st.set_dialog(Dialog::AccOptions("accessibility-checker".into())),
    "accReportView.options" => |st| st.set_dialog(Dialog::AccOptions("accessibility-report".into())),
    "chekerView.options" => |st| st.set_dialog(Dialog::AccOptions("accessibility-checker".into())),
    "chekerView.explain" => explain,
    "chekerView.fail" => |st| verdict(st, Verdict::Failed),
    "chekerView.pass" => |st| verdict(st, Verdict::Passed),
    "chekerView.skip" => |st| verdict(st, Verdict::Skipped),
    "chekerView.fix" => fix_selected,
    "chekerView.reCheck" => run_check,
    "chekerView.showReport" => |st| show_pane(st, Pane::AccReport),
    "chekerView.show.inTags" => |st| show_in(st, Pane::Tags),
    "chekerView.show.inContent" => |st| show_in(st, Pane::Content),
    "chekerView.show.inFields" => |st| show_in(st, Pane::Fields),
    "chekerView.show.inLinks" => |st| show_in(st, Pane::Links),
    "chekerView.show.inComments" => |st| show_in(st, Pane::Comments),
    "contentsView.options" => |st| st.set_dialog(Dialog::AccOptions("content".into())),
    "contentsView.showInTags" => content_in_tags,
    "orderView.options" => |st| st.set_dialog(Dialog::AccOptions("order".into())),
    "orderview.groupsimilar" => group_similar,
    "orderview.show.tableCells" => |st| flip_value(st, "order.cells"),
    "orderview.show.tablesAndFigures" => |st| flip_value(st, "order.tf"),
    "orderview.showmode.none" => |st| set_mode(st, "none"),
    "orderview.showmode.order" => |st| set_mode(st, "order"),
    "orderview.showmode.type" => |st| set_mode(st, "type"),
    "orderview.tag.artifact" => |st| order_tag(st, "Artifact"),
    "orderview.tag.caption" => |st| order_tag(st, "Caption"),
    "orderview.tag.cell" => |st| order_tag(st, "TD"),
    "orderview.tag.field" => |st| order_tag(st, "Form"),
    "orderview.tag.figure" => |st| order_tag(st, "Figure"),
    "orderview.tag.formula" => |st| order_tag(st, "Formula"),
    "orderview.tag.h1" => |st| order_tag(st, "H1"),
    "orderview.tag.h2" => |st| order_tag(st, "H2"),
    "orderview.tag.h3" => |st| order_tag(st, "H3"),
    "orderview.tag.h4" => |st| order_tag(st, "H4"),
    "orderview.tag.h5" => |st| order_tag(st, "H5"),
    "orderview.tag.h6" => |st| order_tag(st, "H6"),
    "orderview.tag.listItem" => |st| order_tag(st, "LI"),
    "orderview.tag.note" => |st| order_tag(st, "Note"),
    "orderview.tag.p" => |st| order_tag(st, "P"),
    "orderview.tag.ref" => |st| order_tag(st, "Reference"),
    "orderview.tag.table" => |st| order_tag(st, "Table"),
    "pageStructure.clear" => |st| {
        if !no_doc(st) {
            confirm(st, "acc.clear.title", "acc.clear.body", ConfirmAction::ClearStructure);
        }
    },
    "pageStructure.deleteItem" => delete_tag,
    "tagsView.createRoot" => create_root,
    "tagsView.options" => |st| st.set_dialog(Dialog::AccOptions("tags".into())),
    "tagsview.applyRoleMap" => |st| count_op(st, "toast.roles_applied", |t| t.doc.apply_role_map()),
    "tagsview.changeToArtifact" => change_to_artifact,
    "tagsview.copyContents" => copy_contents,
    "tagsview.createTagFromSelection" => tag_from_selection,
    "tagsview.deleteEmpty" => |st| count_op(st, "toast.empty_tags_deleted", |t| t.doc.delete_empty_struct()),
    "tagsview.editClassMap" => |st| if need_tags(st).is_some() { st.set_dialog(Dialog::ClassMap) },
    "tagsview.editRoleMap" => |st| if !no_doc(st) { st.set_dialog(Dialog::RoleMap) },
    "tagsview.findTagFromSelection" => find_from_selection,
    "tagsview.isTagged" => is_tagged,
    "tagsview.new" => |st| new_tag(st, false),
    "tagsview.newChild" => |st| new_tag(st, true),
    "tagsview.newSibling" => new_sibling,
    "tagsview.selectTagContent" => |st| if let Some((_, id)) = need_selected(st) { show_tag_content(st, id, true) },
    "tagsview.showInContent" => show_in_content,
}
