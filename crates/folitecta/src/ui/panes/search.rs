//! Search pane: running a search, its options, the grouped results and moving between them.

use super::*;

/// Search the active document and show the first result.
pub fn run_search(st: AppState, query: &str) {
    let q = query.to_string();
    let (opts, proximity, scopes) = crate::commands::search::search_settings(st);
    let found = st.with_active(|t| {
        t.search.query = q.clone();
        let empty = q.trim().is_empty();
        t.search.hits = if empty || !scopes.page_text {
            Vec::new()
        } else {
            t.doc.search_words(&q, opts, proximity)
        };
        t.search.other = if empty {
            Vec::new()
        } else {
            t.doc.search_other(&q, opts, proximity, &scopes)
        };
        t.search.current = 0;
        t.search.collapsed.clear();
        (t.search.hits.len(), t.search.other.len())
    });
    match found {
        Some((0, 0)) if !q.trim().is_empty() => {
            st.toast(ToastKind::Info, t(st.lang(), "toast.not_found"), "")
        }
        Some((n, _)) if n > 0 => reveal_hit(st, 0),
        _ => {}
    }
}

/// Result group of a match outside the page text.
pub fn other_group(f: &FoundIn) -> &'static str {
    match f {
        FoundIn::Comment { .. } => "comments",
        FoundIn::Bookmark { .. } => "bookmarks",
        FoundIn::Field { .. } => "fields",
        FoundIn::DocInfo { .. } => "docinfo",
        FoundIn::Attachment { .. } | FoundIn::AttachmentPage { .. } => "attachments",
        FoundIn::Link { .. } => "links",
    }
}

pub(super) fn other_icon(f: &FoundIn) -> &'static str {
    match f {
        FoundIn::Comment { .. } => "comment",
        FoundIn::Bookmark { .. } => "bookmark",
        FoundIn::Field { .. } => "field",
        FoundIn::DocInfo { .. } => "info",
        FoundIn::Attachment { .. } | FoundIn::AttachmentPage { .. } => "attachment",
        FoundIn::Link { .. } => "link",
    }
}

/// Where a match outside the page text is, for its result line.
pub(super) fn other_place(lang: crate::i18n::Lang, f: &FoundIn, types: bool) -> String {
    let page = |p: usize| format!("{} {}", t(lang, "status.page"), p + 1);
    match f {
        FoundIn::Comment { page: p, subtype, .. } => {
            if types {
                format!("{} · {}", crate::i18n::annot_name(lang, subtype), page(*p))
            } else {
                page(*p)
            }
        }
        FoundIn::Bookmark { page: p, .. } => p.map(page).unwrap_or_default(),
        FoundIn::Field { name, .. } => name.clone(),
        FoundIn::DocInfo { key } => ui_label(lang, "docinfo", key, key),
        FoundIn::Attachment { name } => name.clone(),
        FoundIn::AttachmentPage { name, page: p } => format!("{name} · {}", page(*p)),
        FoundIn::Link { page: p, .. } => page(*p),
    }
}

/// Make search result `i` current and scroll it into view.
pub fn reveal_hit(st: AppState, i: usize) {
    let page = st
        .with_active(|t| {
            t.search.current = i.min(t.search.hits.len().saturating_sub(1));
            t.search.hits.get(t.search.current).map(|h| h.page)
        })
        .flatten();
    let Some(page) = page else { return };
    if st.current_page() != page {
        st.navigate(page, true);
    }
    spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        let _ = document::eval("const h = document.querySelector('#canvas .hit.current'); if (h) { h.scrollIntoView({ block: 'center', inline: 'nearest' }); }").await;
    });
}

/// Go to the next or previous search result (searching first when needed).
pub fn step_search(st: AppState, forward: bool) {
    let (n, cur, query) = st
        .read_active(|t| (t.search.hits.len(), t.search.current, t.search.query.clone()))
        .unwrap_or((0, 0, String::new()));
    if n == 0 {
        if query.trim().is_empty() {
            crate::commands::show_pane(st, Pane::Search);
        } else {
            run_search(st, &query);
        }
        return;
    }
    let next = if forward { (cur + 1) % n } else { (cur + n - 1) % n };
    reveal_hit(st, next);
}

// ---- Strip ----------------------------------------------------------------------

/// A row of the search results.
#[derive(Clone, PartialEq)]
pub(super) enum ResultRow {
    Header {
        key: String,
        label: String,
        count: usize,
        open: bool,
        icon: &'static str,
    },
    Hit {
        i: usize,
        page: usize,
        snippet: String,
        context: String,
    },
    Other {
        i: usize,
        icon: &'static str,
        place: String,
        snippet: String,
    },
    File {
        i: usize,
        place: String,
        snippet: String,
    },
}

#[component]
pub(super) fn SearchPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some((search, contexts)) = st.read_active(|t| {
        // Bookmark titles of the pages with hits.
        let mut pages: Vec<usize> = t.search.hits.iter().map(|h| h.page).collect();
        pages.dedup();
        let contexts: std::collections::HashMap<usize, String> =
            if option_on(st, "searchView.results.showContextBookmarks") {
                pages
                    .iter()
                    .map(|p| (*p, t.doc.bookmark_title_for_page(*p)))
                    .collect()
            } else {
                Default::default()
            };
        (t.search.clone(), contexts)
    }) else {
        return rsx! {};
    };
    let mut query = use_signal(|| search.query.clone());
    // A search started elsewhere (menus, other panes) shows its query here.
    let searched = search.query.clone();
    use_effect(use_reactive!(|searched| {
        if *query.peek() != searched {
            query.set(searched);
        }
    }));
    let mut more = use_signal(|| false);
    let n = search.hits.len();
    let total = n + search.other.len();
    let results_label = tf(lang, "search.results", &[("count", &total.to_string())]);
    let show_page = option_on(st, "searchView.results.showPageNum");
    let wrap = option_on(st, "searchView.results.wrapText");
    let icons = option_on(st, "searchView.results.showIcon");
    let types = option_on(st, "searchView.results.showCommentTypes");
    let mark_open = (st.open_dropdown)().as_deref() == Some("search:mark");
    let (mx, my) = (st.dropdown_pos)();
    let reg = registry();
    let label = |id: &str| {
        reg.get(id)
            .map(|c| cmd_label(lang, c))
            .unwrap_or_else(|| id.to_string())
    };
    let checked = |id: &str| crate::commands::toggle_state(st, id).unwrap_or(false);
    let prox = st.ui.read().value("search.proximity").unwrap_or("").to_string();

    // Result rows, grouped by page, by place and by file.
    let mut rows: Vec<ResultRow> = Vec::new();
    let mut i = 0;
    while i < n {
        let page = search.hits[i].page;
        let j = (i..n).find(|k| search.hits[*k].page != page).unwrap_or(n);
        let key = format!("p{page}");
        let open = !search.collapsed.contains(&key);
        rows.push(ResultRow::Header {
            key,
            label: format!("{} {}", t(lang, "status.page"), page + 1),
            count: j - i,
            open,
            icon: "page",
        });
        if open {
            for k in i..j {
                let h = &search.hits[k];
                rows.push(ResultRow::Hit {
                    i: k,
                    page,
                    snippet: h.snippet.clone(),
                    context: contexts.get(&page).cloned().unwrap_or_default(),
                });
            }
        }
        i = j;
    }
    for group in [
        "comments",
        "bookmarks",
        "fields",
        "docinfo",
        "attachments",
        "links",
    ] {
        let items: Vec<usize> = (0..search.other.len())
            .filter(|k| other_group(&search.other[*k].found_in) == group)
            .collect();
        if items.is_empty() {
            continue;
        }
        let open = !search.collapsed.contains(group);
        rows.push(ResultRow::Header {
            key: group.to_string(),
            label: t(lang, &format!("search.group.{group}")),
            count: items.len(),
            open,
            icon: other_icon(&search.other[items[0]].found_in),
        });
        if open {
            for k in items {
                let o = &search.other[k];
                rows.push(ResultRow::Other {
                    i: k,
                    icon: other_icon(&o.found_in),
                    place: other_place(lang, &o.found_in, types),
                    snippet: o.snippet.clone(),
                });
            }
        }
    }
    let folder = search.folder.clone();
    if let Some(f) = &folder {
        let mut k = 0;
        while k < f.hits.len() {
            let path = f.hits[k].path.clone();
            let j = (k..f.hits.len())
                .find(|x| f.hits[*x].path != path)
                .unwrap_or(f.hits.len());
            let key = path.display().to_string();
            let open = !search.collapsed.contains(&key);
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            rows.push(ResultRow::Header {
                key,
                label: name,
                count: j - k,
                open,
                icon: "open",
            });
            if open {
                for x in k..j {
                    rows.push(ResultRow::File {
                        i: x,
                        place: format!("{} {}", t(lang, "status.page"), f.hits[x].page + 1),
                        snippet: f.hits[x].snippet.clone(),
                    });
                }
            }
            k = j;
        }
    }
    let folder_status = folder.as_ref().map(|f| {
        let key = if f.running {
            "search.folder.running"
        } else {
            "search.folder.done"
        };
        tf(
            lang,
            key,
            &[
                ("done", &f.searched.to_string()),
                ("files", &f.files.to_string()),
                ("count", &f.hits.len().to_string()),
                ("folder", &f.root.display().to_string()),
            ],
        )
    });
    let running = folder.as_ref().is_some_and(|f| f.running);
    let scope_ids = [
        "searchView.includePageText",
        "searchView.includeAnnotations",
        "searchView.includeBookmarks",
        "searchView.includeFormFields",
        "searchView.includeDocInfo",
        "searchView.includeAttachments",
        "searchView.includeExtLinks",
    ];
    let prox_opts = [
        ("", t(lang, "search.words.phrase")),
        ("adjacent", label("searchView.proximity.adjacent")),
        ("paragraph", label("searchView.proximity.samePara")),
        ("page", label("searchView.proximity.samePage")),
        ("document", label("searchView.proximity.sameDoc")),
    ];
    let more_label = if more() {
        t(lang, "search.fewer")
    } else {
        t(lang, "search.more")
    };
    rsx! {
        div { class: "search-box",
            input { placeholder: "{t(lang, \"search.placeholder\")}", value: "{query}", autofocus: true,
                oninput: move |e| query.set(e.value()),
                onkeydown: move |e| {
                    e.stop_propagation();
                    if e.key() == Key::Enter {
                        let q = query();
                        let same = st.read_active(|t| t.search.query == q && !t.search.hits.is_empty()).unwrap_or(false);
                        if same { step_search(st, !e.modifiers().shift()); } else { run_search(st, &q); }
                    } else if e.key() == Key::Escape {
                        query.set(String::new());
                        run_search(st, "");
                    }
                }
            }
            button { class: "tbtn primary", title: "{t(lang, \"search.go\")}", onclick: move |_| run_search(st, &query()), Icon { name: "search" } }
        }
        div { class: "search-opts",
            label { class: "check", input { r#type: "checkbox", checked: search.case_sensitive, onchange: move |_| { st.with_active(|t| t.search.query = query()); dispatch(st, "searchView.caseSensitive"); } } span { "{t(lang, \"search.case\")}" } }
            label { class: "check", input { r#type: "checkbox", checked: search.whole_words, onchange: move |_| { st.with_active(|t| t.search.query = query()); dispatch(st, "searchView.wholeWords"); } } span { "{t(lang, \"search.whole\")}" } }
            label { class: "check", input { r#type: "checkbox", checked: checked("searchView.ignoreDiacritics"), onchange: move |_| { st.with_active(|t| t.search.query = query()); dispatch(st, "searchView.ignoreDiacritics"); } } span { "{t(lang, \"search.diacritics\")}" } }
            button { class: "linkish", onclick: move |_| more.set(!more()), "{more_label}" }
        }
        if more() {
            div { class: "search-more",
                div { class: "search-more-title", "{t(lang, \"search.look_in\")}" }
                div { class: "search-scopes",
                    for id in scope_ids {
                        label { class: "check", key: "{id}",
                            input { r#type: "checkbox", checked: checked(id), onchange: move |_| { st.with_active(|t| t.search.query = query()); dispatch(st, id); } }
                            span { "{label(id)}" }
                        }
                    }
                }
                div { class: "field",
                    label { "{t(lang, \"search.words\")}" }
                    select {
                        onchange: move |e| {
                            let v = e.value();
                            st.with_active(|t| t.search.query = query());
                            st.set_value("search.proximity", v.as_str());
                            crate::commands::search::rerun(st);
                        },
                        for (v, text) in prox_opts {
                            option { key: "{v}", value: "{v}", selected: prox == v, "{text}" }
                        }
                    }
                }
                div { class: "search-more-title", "{t(lang, \"search.folder\")}" }
                div { class: "row", style: "gap:6px;flex-wrap:wrap",
                    button { class: "tbtn", disabled: running,
                        onclick: move |_| { st.with_active(|t| t.search.query = query()); crate::commands::search::search_folder(st); },
                        Icon { name: "open" } "{t(lang, \"search.folder.choose\")}"
                    }
                    if running {
                        button { class: "tbtn", onclick: move |_| { st.with_active(|t| { if let Some(f) = t.search.folder.as_mut() { f.running = false; } }); }, "{t(lang, \"search.folder.stop\")}" }
                    }
                }
                label { class: "check", input { r#type: "checkbox", checked: option_on(st, "searchView.lookInSubFolders"), onchange: move |_| dispatch(st, "searchView.lookInSubFolders") } span { "{label(\"searchView.lookInSubFolders\")}" } }
                label { class: "check", input { r#type: "checkbox", checked: option_on(st, "searchView.estimateFolder"), onchange: move |_| dispatch(st, "searchView.estimateFolder") } span { "{label(\"searchView.estimateFolder\")}" } }
            }
        }
        if total > 0 {
            div { class: "search-nav",
                span { class: "muted ellipsis", "{results_label}" }
                div { class: "spacer" }
                if n > 0 {
                    button { class: "ibtn", title: "{t(lang, \"search.prev\")}", onclick: move |_| step_search(st, false), Icon { name: "prev" } }
                    span { class: "muted", "{search.current + 1}/{n}" }
                    button { class: "ibtn", title: "{t(lang, \"search.next\")}", onclick: move |_| step_search(st, true), Icon { name: "next" } }
                    button { class: if mark_open { "ibtn active" } else { "ibtn" }, title: "{t(lang, \"search.mark\")}",
                        onclick: move |evt| {
                            evt.stop_propagation();
                            if st.open_dropdown.peek().as_deref() == Some("search:mark") {
                                st.set_open_dropdown(None);
                            } else {
                                let c = evt.client_coordinates();
                                let pos = st.clamp_menu_pos(c.x - 20.0, c.y + 16.0, MENU_W, 170.0);
                                st.set_dropdown_pos(pos);
                                st.set_open_dropdown(Some("search:mark".into()));
                            }
                        },
                        Icon { name: "highlight" }
                    }
                }
            }
        }
        if let Some(s) = folder_status {
            div { class: "search-folder-status muted", "{s}" }
        }
        if mark_open {
            div { class: "rbtn-menu fixed", style: "left:{mx}px;top:{my}px", onclick: move |e| e.stop_propagation(),
                for id in ["addAnnotsBySearchResult.highlight", "addAnnotsBySearchResult.underline", "addAnnotsBySearchResult.strikeout", "addAnnotsBySearchResult.redaction"] {
                    {
                        let label = registry().get(id).map(|c| cmd_label(lang, c)).unwrap_or_else(|| id.to_string());
                        rsx! { div { class: "menu-item", key: "{id}", onclick: move |_| { st.set_open_dropdown(None); dispatch(st, id); }, span { class: "label", "{label}" } } }
                    }
                }
            }
        }
        div { class: if wrap { "search-results wrap" } else { "search-results" },
            for (r, row) in rows.into_iter().enumerate() {
                match row {
                    ResultRow::Header { key, label, count, open, icon } => {
                        let k = key.clone();
                        rsx! {
                            div { class: "result-group", key: "g{r}-{key}",
                                onclick: move |_| { let k = k.clone(); st.with_active(|t| { if !t.search.collapsed.remove(&k) { t.search.collapsed.insert(k); } }); },
                                span { class: "twisty", if open { "▾" } else { "▸" } }
                                if icons { Icon { name: icon } }
                                span { class: "ellipsis", title: "{label}", "{label}" }
                                span { class: "chip", "{count}" }
                            }
                        }
                    }
                    ResultRow::Hit { i, page, snippet, context } => {
                        let class = if i == search.current { "list-item result selected" } else { "list-item result" };
                        rsx! {
                            div { class: "{class}", key: "h{i}",
                                onclick: move |_| reveal_hit(st, i),
                                if show_page || !context.is_empty() {
                                    div { class: "title",
                                        if show_page { span { "{t(lang, \"status.page\")} {page + 1}" } }
                                        if !context.is_empty() { span { class: "muted ellipsis", title: "{context}", "{context}" } }
                                    }
                                }
                                div { class: "body snippet", "{snippet}" }
                            }
                        }
                    }
                    ResultRow::Other { i, icon, place, snippet } => {
                        let hit = search.other[i].clone();
                        rsx! {
                            div { class: "list-item result", key: "o{i}",
                                onclick: move |_| crate::commands::search::open_other(st, hit.clone()),
                                div { class: "title",
                                    if icons { Icon { name: icon } }
                                    span { class: "ellipsis", title: "{place}", "{place}" }
                                }
                                div { class: "body snippet", "{snippet}" }
                            }
                        }
                    }
                    ResultRow::File { i, place, snippet } => {
                        let hit = folder.as_ref().and_then(|f| f.hits.get(i).cloned());
                        rsx! {
                            div { class: "list-item result", key: "f{i}",
                                onclick: move |_| { if let Some(h) = hit.clone() { crate::commands::search::open_folder_hit(st, h); } },
                                if show_page { div { class: "title", "{place}" } }
                                div { class: "body snippet", "{snippet}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
