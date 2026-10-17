//! Navigation dialogs: bookmarks, named destinations, going to a page and the document properties.

use super::*;

#[component]
pub(super) fn NamedDestEditDialog(name: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let dest = st
        .read_active(|t| t.doc.named_destinations().into_iter().find(|d| d.name == name))
        .flatten();
    let Some(dest) = dest else {
        return rsx! {};
    };
    let original_page = dest.page;
    let new_name = use_signal(|| dest.name.clone());
    let page_s = use_signal(|| dest.page.map(|p| (p + 1).to_string()).unwrap_or_default());
    let old = name.clone();
    let count = st.page_count();
    let view_text = dest
        .view
        .as_ref()
        .map(|v| v.describe())
        .unwrap_or_else(|| "—".into());
    rsx! {
        Frame { title: t(lang, "cmd.pdfNamedDestsView.edit"),
            on_ok: move |_| {
                let n = new_name().trim().to_string();
                let page: Option<usize> = page_s().trim().parse::<usize>().ok().filter(|p| *p >= 1 && *p <= count).map(|p| p - 1);
                let old = old.clone();
                let r = st.with_active(|t| -> folitecta_core::Result<()> {
                    if n != old {
                        t.doc.rename_named_destination(&old, &n)?;
                    }
                    if let Some(p) = page.filter(|p| Some(*p) != original_page) {
                        t.doc.set_named_destination(&n, p, None)?;
                    }
                    Ok(())
                });
                st.with_active(|t| t.selected_dest = Some(n.clone()));
                match r {
                    Some(Err(e)) => st.error(e),
                    _ => {
                        files::set_window_title(st);
                        st.set_dialog(Dialog::None);
                    }
                }
            },
            Text { label: t(lang, "dialog.dest.name"), value: new_name, autofocus: true }
            Num { label: t(lang, "status.page"), value: page_s, min: "1", max: "{count}" }
            div { class: "kv", span { class: "k", "{t(lang, \"dialog.dest.view\")}" } span { class: "v", "{view_text}" } }
        }
    }
}

#[component]
pub(super) fn BookmarkOptionsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let reg = registry();
    let groups: [(&str, &[&str]); 3] = [
        (
            "dialog.bookmarks.create",
            &[
                "bookmarksView.useTextForTitle",
                "bookmarksView.useCurrentPage",
                "bookmarksView.useInheritedZoom",
                "bookmarksView.useRegionForDest",
            ],
        ),
        (
            "dialog.bookmarks.view",
            &[
                "bookmarksView.showIcons",
                "bookmarksView.wrapTitles",
                "bookmarksView.showContrastText",
                "bookmarksView.markInvalid",
                "bookmarksView.autoExpandBookmarks",
                "bookmarksView.ensureVisibility",
            ],
        ),
        (
            "dialog.bookmarks.dests",
            &[
                "pdfNamedDestsView.useTextForTitle",
                "pdfNamedDestsView.useInheritedZoom",
                "pdfNamedDestsView.useRegionForDest",
                "pdfNamedDestsView.wrapTitles",
                "pdfNamedDestsView.hdr.page",
                "pdfNamedDestsView.hdr.type",
                "pdfNamedDestsView.hdr.details",
            ],
        ),
    ];
    let size = st
        .ui
        .read()
        .value("bookmarks.text_size")
        .unwrap_or("medium")
        .to_string();
    rsx! {
        Frame { title: t(lang, "cmd.bookmarksView.options").trim_end_matches('…').to_string(), wide: true,
            div { class: "field-grid",
                for (title, ids) in groups {
                    div { class: "field", key: "{title}",
                        label { "{t(lang, title)}" }
                        div { class: "check-list",
                            for id in ids.iter().copied() {
                                {
                                    let on = option_on(st, id);
                                    let label = reg.get(id).map(|c| cmd_label(lang, c)).unwrap_or_else(|| id.to_string());
                                    rsx! { label { class: "check", key: "{id}", input { r#type: "checkbox", checked: on, onchange: move |_| st.set_option(id, !on) } span { "{label}" } } }
                                }
                            }
                        }
                    }
                }
                div { class: "field",
                    label { "{t(lang, \"cmd.bookmarksView.textSize\")}" }
                    select { onchange: move |e| st.set_value("bookmarks.text_size", e.value()),
                        for (v, key) in [("small", "cmd.bookmarksView.textSize.small"), ("medium", "cmd.bookmarksView.textSize.medium"), ("large", "cmd.bookmarksView.textSize.large")] {
                            option { key: "{v}", value: "{v}", selected: size == v, "{t(lang, key)}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn BookmarkImportExportDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let has_sel = crate::commands::bookmarks::selected_path(st).is_some();
    let run = move |id: &'static str| {
        move |_| {
            st.set_dialog(Dialog::None);
            crate::commands::dispatch(st, id);
        }
    };
    let copy = move |_| {
        let text = st
            .read_active(|t| t.doc.export_bookmarks_text(None))
            .unwrap_or_default();
        crate::clipboard::set_text(&text);
        st.set_dialog(Dialog::None);
        st.toast(ToastKind::Ok, t(st.lang(), "toast.copied"), "");
    };
    rsx! {
        Frame { title: t(lang, "cmd.bookmarksView.importExport"),
            div { class: "hint", "{t(lang, \"dialog.bookmarks.format\")}" }
            div { class: "button-col",
                button { class: "tbtn", onclick: run("document.exportBookmarks"), Icon { name: "export" } "{t(lang, \"cmd.document.exportBookmarks\")}" }
                button { class: "tbtn", disabled: !has_sel, onclick: run("document.exportSelBookmarks"), Icon { name: "export" } "{t(lang, \"cmd.document.exportSelBookmarks\")}" }
                button { class: "tbtn", onclick: run("document.importBookmarks"), Icon { name: "import" } "{t(lang, \"cmd.document.importBookmarks\")}" }
                button { class: "tbtn", onclick: copy, Icon { name: "copy" } "{t(lang, \"dialog.bookmarks.copy\")}" }
                button { class: "tbtn", onclick: run("pasteBookmarks"), Icon { name: "clipboard" } "{t(lang, \"cmd.pasteBookmarks\")}" }
            }
        }
    }
}

#[component]
pub(super) fn BookmarkPropsDialog(path: Vec<usize>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let item = st
        .read_active(|t| folitecta_core::outline::item_at(&t.doc.outline(), &path).cloned())
        .flatten();
    let Some(item) = item else {
        return rsx! {};
    };
    let count = st.page_count();
    let title = use_signal(|| item.title.clone());
    let mut kind = use_signal(|| {
        if item.page.is_none() && !item.action.is_empty() {
            "uri".to_string()
        } else {
            "page".to_string()
        }
    });
    let page_s = use_signal(|| {
        item.page
            .map(|p| (p + 1).to_string())
            .unwrap_or_else(|| "1".into())
    });
    let uri = use_signal(|| item.action.clone());
    let mut view = use_signal(|| "keep".to_string());
    let mut color = use_signal(|| {
        item.color
            .map(crate::util::color_hex)
            .unwrap_or_else(|| "#000000".into())
    });
    let mut has_color = use_signal(|| item.color.is_some());
    let mut bold = use_signal(|| item.bold);
    let mut italic = use_signal(|| item.italic);
    let original_page = item.page;
    let original_uri = item.action.clone();
    let p = path.clone();
    rsx! {
        Frame { title: t(lang, "dialog.bookmark.props"), wide: true,
            on_ok: move |_| {
                use folitecta_core::{BookmarkTarget, DestView};
                let page: Option<usize> = page_s().trim().parse::<usize>().ok().filter(|v| *v >= 1 && *v <= count).map(|v| v - 1);
                let target = if kind() == "uri" {
                    if uri().trim() == original_uri.trim() { BookmarkTarget::Keep } else { BookmarkTarget::Uri(uri()) }
                } else {
                    match (page, view().as_str()) {
                        (Some(pg), "keep") if Some(pg) == original_page => BookmarkTarget::Keep,
                        (Some(pg), "fit") => BookmarkTarget::Page(pg, Some(DestView::Fit)),
                        (Some(pg), "width") => BookmarkTarget::Page(pg, Some(DestView::FitWidth(None))),
                        (Some(pg), _) => BookmarkTarget::Page(pg, None),
                        (None, _) => BookmarkTarget::Keep,
                    }
                };
                let c = has_color().then(|| parse_color(&color()));
                let (ti, b, i, pp) = (title(), bold(), italic(), p.clone());
                apply(st, move |tab| tab.doc.set_bookmark_properties(&pp, &ti, target, c, b, i));
                st.with_active(|t| t.selected_bookmark = Some(p.clone()));
            },
            Text { label: t(lang, "dialog.bookmark.name"), value: title, autofocus: true }
            div { class: "field",
                label { "{t(lang, \"dialog.bookmark.target\")}" }
                div { class: "radio-row",
                    label { input { r#type: "radio", name: "bm-kind", checked: kind() == "page", onchange: move |_| kind.set("page".into()) } "{t(lang, \"status.page\")}" }
                    label { input { r#type: "radio", name: "bm-kind", checked: kind() == "uri", onchange: move |_| kind.set("uri".into()) } "{t(lang, \"dialog.bookmark.web\")}" }
                }
            }
            if kind() == "uri" {
                Text { label: t(lang, "dialog.link.url"), value: uri, placeholder: "https://" }
            } else {
                div { class: "field-grid",
                    Num { label: t(lang, "status.page"), value: page_s, min: "1", max: "{count}" }
                    div { class: "field",
                        label { "{t(lang, \"dialog.dest.view\")}" }
                        select { onchange: move |e| view.set(e.value()),
                            option { value: "keep", selected: view() == "keep", "{t(lang, \"dialog.bookmark.view_keep\")}" }
                            option { value: "top", selected: view() == "top", "{t(lang, \"dialog.bookmark.view_top\")}" }
                            option { value: "fit", selected: view() == "fit", "{t(lang, \"cmd.zoom.fitPage\")}" }
                            option { value: "width", selected: view() == "width", "{t(lang, \"cmd.zoom.fitWidth\")}" }
                        }
                    }
                }
            }
            div { class: "style-row",
                label { "{t(lang, \"style.text_color\")}" }
                input { r#type: "color", value: "{color}", disabled: !has_color(), onchange: move |e| color.set(e.value()) }
                label { class: "check mini", input { r#type: "checkbox", checked: !has_color(), onchange: move |_| has_color.set(!has_color()) } span { "{t(lang, \"style.none\")}" } }
            }
            div { class: "check-row",
                label { class: "check mini", input { r#type: "checkbox", checked: bold(), onchange: move |_| bold.set(!bold()) } span { "{t(lang, \"cmd.bookmarks.bold\")}" } }
                label { class: "check mini", input { r#type: "checkbox", checked: italic(), onchange: move |_| italic.set(!italic()) } span { "{t(lang, \"cmd.bookmarks.italic\")}" } }
            }
        }
    }
}

#[component]
pub(super) fn DocProps() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let info = st.read_active(|t| t.doc.info()).unwrap_or_default();
    let title = use_signal(|| info.title.clone());
    let author = use_signal(|| info.author.clone());
    let subject = use_signal(|| info.subject.clone());
    let keywords = use_signal(|| info.keywords.clone());
    let path = info
        .path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| t(lang, "label.untitled"));
    let yes_no = |b: bool| {
        if b {
            t(lang, "label.yes")
        } else {
            t(lang, "label.no")
        }
    };
    rsx! {
        Frame { title: t(lang, "dialog.props.title"), wide: true,
            on_ok: move |_| apply(st, |tab| tab.doc.set_info(&title(), &author(), &subject(), &keywords())),
            div { class: "field-grid",
                Text { label: t(lang, "dialog.props.title_field"), value: title, autofocus: true }
                Text { label: t(lang, "dialog.props.author"), value: author }
                Text { label: t(lang, "dialog.props.subject"), value: subject }
                Text { label: t(lang, "dialog.props.keywords"), value: keywords }
            }
            div { class: "kv",
                span { class: "k", "{t(lang, \"dialog.props.path\")}" } span { class: "v", "{path}" }
                span { class: "k", "{t(lang, \"dialog.props.pages\")}" } span { class: "v", "{info.page_count}" }
                span { class: "k", "{t(lang, \"dialog.props.size\")}" } span { class: "v", "{crate::util::human_size(info.file_size)}" }
                span { class: "k", "{t(lang, \"dialog.props.version\")}" } span { class: "v", "{info.version}" }
                span { class: "k", "{t(lang, \"dialog.props.created\")}" } span { class: "v", "{info.created}" }
                span { class: "k", "{t(lang, \"dialog.props.modified\")}" } span { class: "v", "{info.modified}" }
                span { class: "k", "{t(lang, \"dialog.props.creator\")}" } span { class: "v", "{info.creator}" }
                span { class: "k", "{t(lang, \"dialog.props.producer\")}" } span { class: "v", "{info.producer}" }
                span { class: "k", "{t(lang, \"dialog.props.encrypted\")}" } span { class: "v", "{yes_no(info.encrypted)}" }
                span { class: "k", "{t(lang, \"dialog.props.form\")}" } span { class: "v", "{yes_no(info.has_form)}" }
                span { class: "k", "{t(lang, \"dialog.props.tagged\")}" } span { class: "v", "{yes_no(info.tagged)}" }
            }
        }
    }
}

#[component]
pub(super) fn GoToPage() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let page = use_signal(|| (st.current_page() + 1).to_string());
    let zoom = use_signal(|| ((crate::commands::current_zoom(st) * 100.0).round() as i32).to_string());
    let count = st.page_count();
    rsx! {
        Frame { title: t(lang, "dialog.goto.title"),
            on_ok: move |_| {
                if let Ok(p) = page().trim().parse::<usize>() { if p >= 1 { st.go_to_page(p - 1); } }
                if let Ok(z) = zoom().trim().parse::<f32>() { if z >= 10.0 { st.with_active(|t| t.zoom = Zoom::Fixed(z / 100.0)); } }
                st.set_dialog(Dialog::None);
            },
            div { class: "field-grid",
                Num { label: format!("{} (1-{count})", t(lang, "status.page")), value: page, min: "1", max: count.to_string() }
                Num { label: format!("{} (%)", t(lang, "status.zoom")), value: zoom, min: "10", max: "800" }
            }
        }
    }
}

#[component]
pub(super) fn BookmarkRename(path: Vec<usize>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let current = st
        .read_active(|t| {
            let flat = t.doc.outline();
            fn find(items: &[folitecta_core::OutlineItem], p: &[usize]) -> Option<String> {
                let (f, r) = p.split_first()?;
                let it = items.get(*f)?;
                if r.is_empty() {
                    Some(it.title.clone())
                } else {
                    find(&it.children, r)
                }
            }
            find(&flat, &path)
        })
        .flatten()
        .unwrap_or_default();
    let title = use_signal(|| current);
    let p = path.clone();
    rsx! {
        Frame { title: t(lang, "cmd.rename"),
            on_ok: move |_| { let ti = title(); let pp = p.clone(); apply(st, |tab| tab.doc.rename_bookmark(&pp, &ti)); },
            Text { label: t(lang, "dialog.bookmark.name"), value: title, autofocus: true }
        }
    }
}
