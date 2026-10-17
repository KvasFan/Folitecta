//! Thumbnails pane: page pictures to select, drag to reorder and act on.

use super::*;

#[component]
pub(super) fn ThumbnailsPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut drag_src = use_signal(|| None::<usize>);
    let mut drag_over = use_signal(|| None::<usize>);
    let mut dragged = use_signal(|| false);
    let Some((tab_id, rev, page, count, sizes, selected, rotation)) = st.read_active(|t| {
        (
            t.id,
            t.view_rev(),
            t.page,
            t.doc.page_count(),
            t.page_sizes.clone(),
            t.selected_pages.clone(),
            t.view_rotation,
        )
    }) else {
        return rsx! {};
    };
    let box_px: f32 = st
        .ui
        .read()
        .value("thumb.size")
        .and_then(|v| v.parse().ok())
        .unwrap_or(150.0f32)
        .clamp(80.0, 320.0);
    let logical = option_on(st, "thumbnailsView.logicalVisible");
    let labels = if logical {
        st.read_active(|t| t.doc.page_labels()).unwrap_or_default()
    } else {
        Vec::new()
    };
    let render_scale = ((box_px * 1.6 / 842.0) * 1000.0).round().clamp(120.0, 900.0) as u32;
    let mut finish_drag = move || {
        let (src, over) = (*drag_src.peek(), *drag_over.peek());
        drag_src.set(None);
        drag_over.set(None);
        if let (Some(s), Some(o)) = (src, over) {
            if s != o {
                let r = st.with_active(|t| {
                    let moving: Vec<usize> = if t.selected_pages.contains(&s) {
                        t.selected_pages.clone()
                    } else {
                        vec![s]
                    };
                    let before = if o > s { o + 1 } else { o };
                    let to = (0..before).filter(|i| !moving.contains(i)).count();
                    let first_new = to;
                    let n = moving.len();
                    let res = t.doc.move_pages(&moving, to);
                    if res.is_ok() {
                        t.selected_pages = (first_new..first_new + n).collect();
                        t.page = first_new;
                    }
                    res
                });
                match r {
                    Some(Err(e)) => st.error(e),
                    _ => files::set_window_title(st),
                }
            }
        }
    };
    rsx! {
        div { class: "thumbs", style: "--thumb:{box_px}px",
            onmouseleave: move |_| { drag_src.set(None); drag_over.set(None); },
            onmouseup: move |_| finish_drag(),
            for p in 0..count {
                {
                    let s = sizes.get(p).copied();
                    let (w, h) = s.map(|s| if rotation % 180 != 0 { (s.height, s.width) } else { (s.width, s.height) }).unwrap_or((595.0, 842.0));
                    let scale = box_px / w.max(h);
                    let (wpx, hpx) = ((w * scale).round() as i32, (h * scale).round() as i32);
                    let is_sel = selected.contains(&p);
                    let is_target = drag_over() == Some(p) && drag_src() != Some(p);
                    let class = format!(
                        "thumb{}{}{}",
                        if p == page { " current" } else { "" },
                        if is_sel { " selected" } else { "" },
                        if is_target { if drag_src().map(|s| s < p).unwrap_or(false) { " drop-after" } else { " drop-before" } } else { "" }
                    );
                    let src = format!("/page/{tab_id}/{p}/{rev}/{render_scale}.png");
                    let label = if logical { labels.get(p).cloned().unwrap_or_else(|| (p + 1).to_string()) } else { (p + 1).to_string() };
                    let img_style = if rotation != 0 { format!("width:{wpx}px;height:{hpx}px;transform:rotate({rotation}deg)") } else { format!("width:{wpx}px;height:{hpx}px") };
                    let (iw, ih) = if rotation % 180 != 0 { (hpx, wpx) } else { (wpx, hpx) };
                    rsx! {
                        div { class: "{class}", key: "{p}", title: "{t(lang, \"status.page\")} {p + 1}",
                            onmousedown: move |e| {
                                if e.trigger_button() == Some(MouseButton::Primary) {
                                    drag_src.set(Some(p));
                                    dragged.set(false);
                                }
                            },
                            onmouseenter: move |_| {
                                if drag_src.peek().is_some() {
                                    drag_over.set(Some(p));
                                    dragged.set(true);
                                }
                            },
                            onclick: move |e| {
                                if *dragged.peek() {
                                    dragged.set(false);
                                    return;
                                }
                                if e.modifiers().ctrl() || e.modifiers().meta() {
                                    st.with_active(|t| { if let Some(i) = t.selected_pages.iter().position(|x| *x == p) { t.selected_pages.remove(i); } else { t.selected_pages.push(p); } });
                                } else if e.modifiers().shift() {
                                    st.with_active(|t| { let (a, b) = (t.page.min(p), t.page.max(p)); t.selected_pages = (a..=b).collect(); });
                                } else {
                                    st.with_active(|t| t.selected_pages = vec![p]);
                                    st.go_to_page(p);
                                }
                            },
                            ondoubleclick: move |_| st.go_to_page(p),
                            div { class: "thumb-img", style: "width:{wpx.max(iw)}px;height:{hpx.max(ih)}px",
                                img { src: "{src}", alt: "", loading: "lazy", draggable: "false", width: "{wpx}", height: "{hpx}", style: "{img_style}" }
                            }
                            span { class: "num", "{label}" }
                        }
                    }
                }
            }
        }
    }
}

// ---- Attachments, layers, fields, links, destinations, signatures ------------------
