//! One page of the canvas: its picture, comments, links, fields, search results, selections and the tools at work on it.

use super::*;

#[component]
pub(super) fn PageView(
    tab_id: u64,
    page: usize,
    rev: u64,
    scale: u32,
    // The scale (milli) of the small picture shown until the sharp one has
    // come (the thumbnails' pictures); 0: none.
    preview: u32,
    zoom: f32,
    current: bool,
    tool: Tool,
    view_rotation: i32,
    label: String,
    // What the page shows besides its picture (see `page_stamps`): the page is
    // drawn again when it changes. The documents are read without following
    // them (`peek_active`), so a change elsewhere does not draw every page.
    stamp: u64,
) -> Element {
    crate::perf::PAGE_VIEWS.bump();
    let _ = stamp;
    let st = use_context::<AppState>();
    let mut drag = use_signal(|| None::<Drag>);
    let mut ink: Signal<Vec<(f64, f64)>> = use_signal(Vec::new);
    let mut annot_drag = use_signal(|| None::<AnnotDrag>);
    let mut sel_drag = use_signal(|| false);
    let mut hover = use_signal(|| None::<(f32, f32)>);

    let show_hits = matches!(tool, Tool::Hand | Tool::EditAnnots);
    let edit_fields = tool == Tool::EditFields;
    let fill_forms = matches!(tool, Tool::Hand | Tool::Select);
    let page_label = if !label.is_empty() && label != (page + 1).to_string() {
        format!("{label} ({})", page + 1)
    } else {
        (page + 1).to_string()
    };
    let highlight_links = option_on(st, "view.highlightLinks");
    let show_guides = option_on(st, "view.showGuides");
    let show_grid = option_on(st, "view.showGrid");
    let rulers = option_on(st, "view.showRulers");
    let transparent = option_on(st, "view.showTranspGrid");
    let dark = option_on(st, "view.darkPageMode");
    let contrast = option_on(st, "view.overrideDocumentColors");
    let track_pointer = option_on(st, "view.showPageMeasureInfo");
    let show_size = option_on(st, "view.showPageSizePopup");
    let colored = option_on(st, "document.find.coloredResults");

    struct View {
        size: PageSize,
        annots: Vec<(usize, String, [f32; 4], String)>,
        links: Vec<[f32; 4]>,
        fields: Vec<[f32; 4]>,
        hits: Vec<(bool, [f32; 4], usize)>,
        sel: Vec<[f32; 4]>,
        caret: Option<[f32; 4]>,
        guides: Vec<(bool, f32)>,
        selected: Option<usize>,
        /// Every comment chosen on this page (the selected one and those
        /// added with Shift or Ctrl).
        chosen: Vec<usize>,
        widgets: Vec<(folitecta_core::PageWidget, [f32; 4])>,
        popups: Vec<PopupView>,
        flash: Option<[f32; 4]>,
        /// Tab order and calculation order numbers of the form fields: label, calculation, rectangle.
        field_marks: Vec<(String, bool, [f32; 4])>,
        /// Reading order boxes: label, selected, rectangle (display space).
        order_boxes: Vec<(String, bool, [f32; 4])>,
        /// Pictures of the page for the pictures tool: index and rectangle (display space).
        images: Vec<(usize, [f32; 4])>,
        /// The picture selected on this page.
        image_sel: Option<usize>,
        /// Words the dictionaries do not know, and the one Spell Check and
        /// Replace shows (rectangles, display space).
        spell_words: Vec<[f32; 4]>,
        spell_shown: Vec<[f32; 4]>,
    }
    // The spell check marks words while its dialog is open, and while the
    // page text is edited with Spell Check on.
    let spell_checking = matches!(*st.dialog.read(), Dialog::SpellCheck);
    let spell_editing = tool == Tool::EditText && option_on(st, "view.spellCheck");
    if spell_editing {
        crate::commands::spelling::ensure_loaded(st);
    }
    let order_mode = {
        let ui = st.ui.read();
        (ui.show_left_pane && ui.left_pane == "order").then(|| crate::commands::accessibility::order_mode(st))
    }
    .filter(|m| m != "none");
    let order_sel = current
        .then(|| st.peek_active(|t| t.acc.order).flatten())
        .flatten();
    let order_cells = st.value("order.cells") == "1";
    let order_only = st.value("order.tf") == "1";
    let show_tabs = option_on(st, "tool.editFields.showTabNumbers");
    let show_calc = option_on(st, "tool.editFields.showCalcOrdIdx");
    let designing = edit_fields || tool.new_field().is_some();
    let Some(view) = st
        .peek_active(|t| {
            let size = t.page_sizes.get(page).copied()?;
            let infos = t.doc.page_comment_infos(page);
            let filtered: std::collections::HashSet<usize> = infos
                .iter()
                .filter(|c| c.reply_to.is_none() && !c.is_state && !t.comment_filter.shows(c))
                .map(|c| c.index)
                .collect();
            let popups = popup_views(&infos, &filtered, &size);
            let annots = if show_hits || edit_fields || t.selected_annot.map(|(p, _)| p) == Some(page) {
                t.doc
                    .annotations(page)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|a| {
                        if edit_fields {
                            a.subtype == "Widget"
                        } else {
                            !["Link", "Widget", "Popup"].contains(&a.subtype.as_str())
                                && !a.hidden
                                && !a.is_reply
                                && !filtered.contains(&a.index)
                        }
                    })
                    .map(|a| {
                        (
                            a.index,
                            a.subtype.clone(),
                            pdf_rect_to_display(&size, a.rect),
                            a.contents.clone(),
                        )
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let links = if highlight_links {
                t.doc
                    .links(page)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|l| pdf_rect_to_display(&size, l.rect))
                    .collect()
            } else {
                Vec::new()
            };
            let fields = if t.highlight_fields {
                t.doc
                    .form_fields()
                    .into_iter()
                    .filter(|f| f.page == Some(page))
                    .filter(|f| crate::commands::forms::highlighted(t, f))
                    .filter_map(|f| f.rect)
                    .map(|r| pdf_rect_to_display(&size, r))
                    .collect()
            } else {
                Vec::new()
            };
            let hits = t
                .search
                .hits
                .iter()
                .enumerate()
                .filter(|(_, h)| h.page == page)
                .flat_map(|(i, h)| h.rects.iter().map(move |r| (i == t.search.current, *r, h.word)))
                .collect();
            let sel = match t.text_sel {
                Some(s) if s.page == page && !s.is_empty() => {
                    let (a, b) = s.range();
                    t.doc
                        .page_text_layer(page)
                        .map(|l| l.range_rects(a, b))
                        .unwrap_or_default()
                }
                _ => Vec::new(),
            };
            // Where typed text goes with the edit text tool.
            let caret = match t.text_sel {
                Some(s) if s.page == page && s.is_empty() && tool == Tool::EditText => {
                    t.doc.page_text_layer(page).and_then(|l| {
                        let glyph = |i: usize| {
                            l.char_glyph
                                .get(i)
                                .copied()
                                .flatten()
                                .and_then(|g| l.glyphs.get(g))
                        };
                        glyph(s.head)
                            .map(|g| [g.bbox[0], g.bbox[1], g.bbox[0] + 1.0, g.bbox[3]])
                            .or_else(|| {
                                s.head
                                    .checked_sub(1)
                                    .and_then(glyph)
                                    .map(|g| [g.bbox[2], g.bbox[1], g.bbox[2] + 1.0, g.bbox[3]])
                            })
                    })
                }
                _ => None,
            };
            let guides = t
                .guides
                .iter()
                .filter(|g| g.page.is_none() || g.page == Some(page))
                .map(|g| (g.horizontal, g.pos))
                .collect();
            let selected = t.selected_annot.filter(|(p, _)| *p == page).map(|(_, i)| i);
            let chosen = t
                .chosen_annots()
                .filter(|(p, _)| *p == page)
                .map(|(_, l)| l)
                .unwrap_or_default();
            let widgets = if fill_forms && t.doc.has_form() {
                t.doc
                    .page_widgets(page)
                    .into_iter()
                    .map(|w| {
                        let r = pdf_rect_to_display(&size, w.rect);
                        (w, r)
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let field_marks = if designing && (show_tabs || show_calc) && t.doc.has_form() {
                let mut ws = t.doc.page_widgets(page);
                let mut out = Vec::new();
                if show_tabs {
                    // Rows: top to bottom then left to right; columns: the other way round.
                    match t.doc.page_tab_order(page).as_str() {
                        "R" => ws.sort_by(|a, b| {
                            b.rect[3]
                                .total_cmp(&a.rect[3])
                                .then(a.rect[0].total_cmp(&b.rect[0]))
                        }),
                        "C" => ws.sort_by(|a, b| {
                            a.rect[0]
                                .total_cmp(&b.rect[0])
                                .then(b.rect[3].total_cmp(&a.rect[3]))
                        }),
                        _ => {}
                    }
                    for (n, w) in ws.iter().enumerate() {
                        out.push(((n + 1).to_string(), false, pdf_rect_to_display(&size, w.rect)));
                    }
                }
                if show_calc {
                    let co = t.doc.calculation_order();
                    for w in &ws {
                        if let Some(k) = co.iter().position(|f| *f == w.field) {
                            out.push((format!("c{}", k + 1), true, pdf_rect_to_display(&size, w.rect)));
                        }
                    }
                }
                out
            } else {
                Vec::new()
            };
            let images = if tool == Tool::EditImages {
                t.doc
                    .page_images(page)
                    .map(|list| {
                        list.iter()
                            .map(|im| (im.index, pdf_rect_to_display(&size, im.rect)))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let (spell_words, spell_shown) =
                crate::commands::spelling::page_marks(t, page, spell_checking, spell_editing);
            Some(View {
                size,
                annots,
                links,
                fields,
                hits,
                spell_words,
                spell_shown,
                sel,
                caret,
                guides,
                selected,
                chosen,
                widgets,
                popups,
                flash: t.flash.filter(|(p, _)| *p == page).map(|(_, r)| r),
                images,
                image_sel: t.image_sel.filter(|(p, _)| *p == page).map(|(_, i)| i),
                field_marks,
                order_boxes: match &order_mode {
                    Some(mode) => t
                        .doc
                        .reading_order(page, order_cells)
                        .into_iter()
                        .filter(|i| {
                            !order_only || matches!(i.role.as_str(), "Table" | "Figure" | "TD" | "TH")
                        })
                        .enumerate()
                        .filter(|(_, i)| i.rect[2] > i.rect[0])
                        .map(|(n, i)| {
                            (
                                if mode == "type" {
                                    i.tag
                                } else {
                                    (n + 1).to_string()
                                },
                                order_sel == Some(n),
                                i.rect,
                            )
                        })
                        .collect(),
                    None => Vec::new(),
                },
            })
        })
        .flatten()
    else {
        return rsx! {};
    };
    let size = view.size;
    let size_text = format!(
        "{:.0} × {:.0} mm ({:.2} × {:.2} in)",
        size.width * 25.4 / 72.0,
        size.height * 25.4 / 72.0,
        size.width / 72.0,
        size.height / 72.0
    );
    let (inner_w, inner_h) = (size.width * zoom, size.height * zoom);
    let (outer_w, outer_h) = if view_rotation % 180 != 0 {
        (inner_h, inner_w)
    } else {
        (inner_w, inner_h)
    };
    let transform = match view_rotation {
        90 => format!("translate({inner_h}px,0) rotate(90deg)"),
        180 => format!("translate({inner_w}px,{inner_h}px) rotate(180deg)"),
        270 => format!("translate(0,{inner_w}px) rotate(270deg)"),
        _ => String::new(),
    };
    // No picture is asked for while the scale is not known (0).
    let src = (scale > 0).then(|| {
        if transparent {
            format!("/page/{tab_id}/{page}/{rev}/{scale}.png?bg=0&lane=view")
        } else {
            format!("/page/{tab_id}/{page}/{rev}/{scale}.png?lane=view")
        }
    });
    // A small picture under the sharp one, seen until it comes (not over a
    // transparent page, where it would hide the grid).
    let preview_src = (scale > 0 && preview > 0 && preview < scale && !transparent)
        .then(|| format!("/page/{tab_id}/{page}/{rev}/{preview}.png?lane=preview"));
    let mut img_filter = String::new();
    if dark {
        img_filter.push_str("invert(1) hue-rotate(180deg) ");
    }
    if contrast {
        img_filter.push_str("grayscale(1) contrast(1.6) ");
    }
    let z = zoom;
    let css = |r: [f32; 4]| -> String {
        format!(
            "left:{}px;top:{}px;width:{}px;height:{}px",
            r[0] * z,
            r[1] * z,
            ((r[2] - r[0]) * z).max(2.0),
            ((r[3] - r[1]) * z).max(2.0)
        )
    };

    // ---- finishing a creation drag -------------------------------------------------
    let finish_create =
        move |d: Drag, points: Vec<(f64, f64)>| finish_create(st, page, size, tool, z, d, points);

    // ---- finishing a move/resize of a comment ------------------------------------
    let finish_annot = move |d: AnnotDrag| {
        let moved = dragged_rect(&d, z, view_rotation);
        if (moved[0] - d.rect[0]).abs() < 0.5
            && (moved[1] - d.rect[1]).abs() < 0.5
            && (moved[2] - d.rect[2]).abs() < 0.5
            && (moved[3] - d.rect[3]).abs() < 0.5
        {
            return;
        }
        // Several comments chosen (a group, or with Shift): they move together.
        let chosen = st
            .read_active(|t| t.chosen_annots().filter(|(p, _)| *p == page).map(|(_, l)| l))
            .flatten()
            .unwrap_or_default();
        if matches!(d.kind, AnnotDragKind::Move) && chosen.len() > 1 && chosen.contains(&d.index) {
            let (dx, dy) = (moved[0] - d.rect[0], moved[1] - d.rect[1]);
            match st.with_active(|t| t.doc.shift_annotations(page, &chosen, dx, dy)) {
                Some(Err(e)) => st.error(e),
                _ => {
                    st.with_active(|t| t.set_chosen(page, &chosen));
                    files::set_window_title(st);
                }
            }
            return;
        }
        let pdf_rect = rect_to_pdf(&size, moved);
        let r = st.with_active(|t| {
            let res = t.doc.set_annotation_rect(page, d.index, pdf_rect);
            t.selected_annot = Some((page, d.index));
            res
        });
        match r {
            Some(Err(e)) => st.error(e),
            _ => {
                st.with_active(|t| t.selected_annot = Some((page, d.index)));
                files::set_window_title(st);
            }
        }
    };

    let mut finish_text = move || {
        sel_drag.set(false);
        let kind = match tool {
            Tool::TextHighlight => Some(Markup::Highlight),
            Tool::TextUnderline => Some(Markup::Underline),
            Tool::TextStrike => Some(Markup::StrikeOut),
            _ => None,
        };
        if let Some(k) = kind {
            let has = st
                .read_active(|t| t.text_sel.map(|s| !s.is_empty()).unwrap_or(false))
                .unwrap_or(false);
            if has {
                commands::markup_selection(st, Some(k));
            }
        }
        // The comment tools suggest the change with a comment; the page text
        // changes with the edit text tool and the Edit commands of Home.
        if matches!(tool, Tool::ReplaceText | Tool::TextCorrection) {
            match st.read_active(|t| t.text_sel).flatten() {
                Some(sel) if !sel.is_empty() => {
                    let (start, end) = sel.range();
                    st.set_dialog(Dialog::ReplaceText {
                        page: sel.page,
                        start,
                        end,
                        note: true,
                    });
                }
                // A click of text correction marks where text goes in.
                Some(sel) if tool == Tool::TextCorrection => st.set_dialog(Dialog::InsertText {
                    page: sel.page,
                    index: sel.head,
                    after: false,
                    note: true,
                }),
                _ => {}
            }
        }
    };

    let page_class = if current { "page current-page" } else { "page" };
    let ink_points: String = ink()
        .iter()
        .map(|(x, y)| format!("{x},{y}"))
        .collect::<Vec<_>>()
        .join(" ");
    // The pencil stroke as the smooth curve it will become.
    let ink_curve = if tool == Tool::Ink {
        smooth_svg_path(&ink())
    } else {
        String::new()
    };
    let cur_drag = annot_drag();
    let units = st.ui.read().value("units").unwrap_or("mm").to_string();
    let (unit_pt, minor, major, unit_name) = unit_info(&units);
    let grid_px = grid_step(st) * z;
    let sel_first = view.sel.first().copied();
    // Differences of a comparison this document is part of.
    let cmp_marks: Vec<(String, [f32; 4])> = {
        let c = st.comparison.read();
        match c.as_ref() {
            Some(c) if c.filter != "none" && (tab_id == c.old_tab || tab_id == c.new_tab) => {
                let old = tab_id == c.old_tab;
                let mut out = Vec::new();
                for (i, d) in c.result.differences.iter().enumerate() {
                    if c.filter == "list" && i != c.current {
                        continue;
                    }
                    let spans = if old { &d.old } else { &d.new };
                    let kind = match d.kind {
                        folitecta_core::compare::DiffKind::Inserted => "ins",
                        folitecta_core::compare::DiffKind::Deleted => "del",
                        folitecta_core::compare::DiffKind::Changed => "chg",
                    };
                    let class = if i == c.current {
                        format!("cmp {kind} current")
                    } else {
                        format!("cmp {kind}")
                    };
                    for s in spans.iter().filter(|s| s.page == page) {
                        for r in &s.rects {
                            let r = if c.rows {
                                [0.0, r[1], view.size.width, r[3]]
                            } else {
                                *r
                            };
                            out.push((class.clone(), r));
                        }
                    }
                }
                out
            }
            _ => Vec::new(),
        }
    };
    // In-progress multi-click shape on this page: points string and vertex dots.
    let drawing_here = (st.drawing)().filter(|(p, _)| *p == page).map(|(_, v)| v);
    let multi_vertices: Vec<(f32, f32)> = drawing_here
        .clone()
        .unwrap_or_default()
        .iter()
        .map(|(x, y)| (x * z, y * z))
        .collect();
    let multi_preview = drawing_here.filter(|v| !v.is_empty()).map(|mut v| {
        if let Some(h) = hover() {
            v.push(h);
        }
        let pts = v
            .iter()
            .map(|(x, y)| format!("{},{}", x * z, y * z))
            .collect::<Vec<_>>()
            .join(" ");
        (pts, matches!(tool, Tool::Polygon | Tool::Cloud | Tool::Area))
    });

    // Mouse moves are listened to only while something follows them (a
    // drag on this page, the loupe, the dynamic zoom, the pointer position,
    // the next point of a shape), by a layer over the page made for that
    // time: a listener of mouse moves makes every move in the window wait
    // for the program. During a drag the layer is above everything on the
    // page, so the moves over comments and fields reach it too.
    let on_move = move |evt: Event<MouseData>| {
        if tool == Tool::ZoomDyn {
            // Up zooms in, down zooms out, while the button is held.
            let start = *crate::commands::fileops::ZOOM_DRAG.peek();
            if let Some((y0, z0)) = start {
                if evt.held_buttons().contains(MouseButton::Primary) {
                    let dy = evt.client_coordinates().y - y0;
                    let nz = crate::commands::fileops::dyn_zoom(z0, dy);
                    st.with_active(|t| t.zoom = Zoom::Fixed(nz));
                } else {
                    *crate::commands::fileops::ZOOM_DRAG.write() = None;
                }
            }
            return;
        }
        let p = evt.element_coordinates();
        let (x, y) = ((p.x as f32) / z, (p.y as f32) / z);
        if track_pointer {
            st.set_pointer(Some((page, x, y)));
        }
        // Without mouse tracking the loupe stays where it was clicked.
        if tool == Tool::Loupe
            && (option_on(st, "tool.loupe.trackMouse")
                || evt
                    .held_buttons()
                    .contains(dioxus::html::input_data::MouseButton::Primary))
        {
            let c = evt.client_coordinates();
            st.set_loupe(Some((page, x, y, c.x, c.y)));
        }
        if *sel_drag.peek() {
            if let Some(i) = st
                .read_active(|t| t.doc.page_text_layer(page).and_then(|l| l.char_at(x, y)))
                .flatten()
            {
                let changed = st
                    .read_active(|t| t.text_sel.map(|s| s.head != i).unwrap_or(true))
                    .unwrap_or(false);
                if changed {
                    st.with_active(|t| {
                        if let Some(s) = t.text_sel.as_mut() {
                            if s.page == page {
                                s.head = i;
                            }
                        }
                    });
                }
            }
        }
        if tool.is_multi_click()
            && st
                .drawing
                .peek()
                .as_ref()
                .map(|(p, _)| *p == page)
                .unwrap_or(false)
        {
            let (sx, sy) = snap(st, page, x, y, z);
            hover.set(Some((sx, sy)));
        }
        let freehand = matches!(tool, Tool::Ink | Tool::FreeHighlight | Tool::Eraser);
        let current_drag: Option<Drag> = *drag.peek();
        if let Some(mut d) = current_drag {
            let (sx, sy) = if freehand { (x, y) } else { snap(st, page, x, y, z) };
            d.current = ((sx * z) as f64, (sy * z) as f64);
            if evt.modifiers().shift() {
                d.current = constrained(tool, d.start, d.current);
            }
            drag.set(Some(d));
            if freehand {
                ink.write().push(d.current);
            }
        }
        let current_annot: Option<AnnotDrag> = *annot_drag.peek();
        if let Some(mut d) = current_annot {
            let c = evt.client_coordinates();
            d.cur_client = (c.x, c.y);
            annot_drag.set(Some(d));
        }
    };
    // With the hand and zoom tools (looking at the pages) nothing follows the
    // moves unless the pointer position is shown.
    let follows_moves = track_pointer || !matches!(tool, Tool::Hand | Tool::Zoom);

    // The handlers of the page area.
    let onmousedown = move |evt: Event<MouseData>| {
        if evt.trigger_button() != Some(MouseButton::Primary) {
            return;
        }
        let p = evt.element_coordinates();
        let (x, y) = ((p.x as f32) / z, (p.y as f32) / z);
        if !current && st.current_page() != page {
            st.with_active(|t| t.page = page);
        }
        if tool == Tool::ZoomDyn {
            let c = evt.client_coordinates();
            *crate::commands::fileops::ZOOM_DRAG.write() = Some((c.y, current_zoom(st)));
            return;
        }
        if tool.is_text() {
            let idx = st
                .read_active(|t| t.doc.page_text_layer(page).and_then(|l| l.char_at(x, y)))
                .flatten();
            match idx {
                Some(i) => {
                    let extend = evt.modifiers().shift();
                    st.with_active(|t| {
                        t.selected_annot = None;
                        match (extend, t.text_sel) {
                            (true, Some(s)) if s.page == page => t.text_sel = Some(TextSel { head: i, ..s }),
                            _ => {
                                t.text_sel = Some(TextSel {
                                    page,
                                    anchor: i,
                                    head: i,
                                })
                            }
                        }
                    });
                    sel_drag.set(true);
                }
                None => {
                    st.with_active(|t| t.text_sel = None);
                }
            }
            return;
        }
        if tool.is_multi_click() {
            let (sx, sy) = snap(st, page, x, y, z);
            let mut pts = st
                .drawing
                .peek()
                .clone()
                .filter(|(p, _)| *p == page)
                .map(|(_, v)| v)
                .unwrap_or_default();
            pts.push((sx, sy));
            st.set_drawing(Some((page, pts)));
            return;
        }
        if tool == Tool::InsertText {
            let hit = st
                .read_active(|t| {
                    let l = t.doc.page_text_layer(page)?;
                    let i = l.char_at(x, y)?;
                    let after = l
                        .char_glyph
                        .get(i)
                        .copied()
                        .flatten()
                        .and_then(|g| l.glyphs.get(g))
                        .map(|g| x > (g.bbox[0] + g.bbox[2]) / 2.0)
                        .unwrap_or(false);
                    Some((i, after))
                })
                .flatten();
            match hit {
                Some((index, after)) => st.set_dialog(Dialog::InsertText {
                    page,
                    index,
                    after,
                    note: true,
                }),
                None => st.toast(ToastKind::Info, t(st.lang(), "toast.click_on_text"), ""),
            }
            return;
        }
        // Written only when there is something to clear: every write of
        // the documents draws the whole view again.
        if tool == Tool::EditAnnots || tool == Tool::Hand {
            if st.read_active(|t| t.selected_annot.is_some()).unwrap_or(false) {
                st.with_active(|t| t.selected_annot = None);
            }
            return;
        }
        if tool == Tool::EditImages {
            if st.read_active(|t| t.image_sel.is_some()).unwrap_or(false) {
                st.with_active(|t| t.image_sel = None);
            }
            return;
        }
        if tool.is_drawing() {
            let (sx, sy) = snap(st, page, x, y, z);
            let start = ((sx * z) as f64, (sy * z) as f64);
            drag.set(Some(Drag {
                start,
                current: start,
            }));
            ink.set(vec![start]);
        }
    };
    let onmouseup = move |_: Event<MouseData>| {
        if crate::commands::fileops::ZOOM_DRAG.peek().is_some() {
            *crate::commands::fileops::ZOOM_DRAG.write() = None;
        }
        if *sel_drag.peek() {
            finish_text();
        }
        let current_drag: Option<Drag> = *drag.peek();
        if let Some(d) = current_drag {
            let points = ink.peek().clone();
            drag.set(None);
            ink.set(Vec::new());
            finish_create(d, points);
        }
        let current_annot: Option<AnnotDrag> = *annot_drag.peek();
        if let Some(d) = current_annot {
            annot_drag.set(None);
            finish_annot(d);
        }
    };
    let onmouseleave = move |_: Event<MouseData>| {
        if *sel_drag.peek() {
            finish_text();
        }
        if drag.peek().is_some() {
            drag.set(None);
            ink.set(Vec::new());
        }
        let current_annot: Option<AnnotDrag> = *annot_drag.peek();
        if let Some(d) = current_annot {
            annot_drag.set(None);
            finish_annot(d);
        }
    };
    let ondoubleclick = move |evt: Event<MouseData>| {
        if tool.is_multi_click() {
            finish_drawing(st);
            return;
        }
        if tool != Tool::Select {
            return;
        }
        let p = evt.element_coordinates();
        let (x, y) = ((p.x as f32) / z, (p.y as f32) / z);
        let word = st
            .read_active(|t| {
                t.doc
                    .page_text_layer(page)
                    .and_then(|l| l.char_at(x, y).map(|i| l.word_at(i.saturating_sub(0))))
            })
            .flatten();
        if let Some((a, b)) = word {
            st.with_active(|t| {
                t.text_sel = Some(TextSel {
                    page,
                    anchor: a,
                    head: b,
                })
            });
        }
    };
    let onclick = move |evt: Event<MouseData>| {
        if tool == Tool::Zoom {
            let z = current_zoom(st);
            let f = if evt.modifiers().shift() { 0.8 } else { 1.25 };
            st.with_active(|t| t.zoom = Zoom::Fixed((z * f).clamp(0.1, 8.0)));
        }
    };
    let inner_class = if transparent {
        "page-inner transparent"
    } else {
        "page-inner"
    };
    let inner_style = format!("width:{inner_w}px;height:{inner_h}px;transform:{transform}");
    let inner_content = rsx! {
                if let Some(psrc) = preview_src {
                    img { class: "page-preview", src: "{psrc}", alt: "", decoding: "async", draggable: "false", style: if img_filter.is_empty() { String::new() } else { format!("filter:{img_filter}") } }
                }
                img { class: "page-img", src, alt: "", decoding: "async", draggable: "false", style: if img_filter.is_empty() { String::new() } else { format!("filter:{img_filter}") } }
                if show_grid {
                    div { class: "grid-overlay", style: "background-size:{grid_px}px {grid_px}px" }
                }
                if show_guides {
                    for (i, (horizontal, pos)) in view.guides.iter().enumerate() {
                        div { key: "g{i}", class: if *horizontal { "guide h" } else { "guide v" }, style: if *horizontal { format!("top:{}px", pos * z) } else { format!("left:{}px", pos * z) } }
                    }
                }
                for (i, r) in view.links.iter().enumerate() {
                    div { key: "l{i}", class: "link-box", style: "{css(*r)}" }
                }
                for (i, r) in view.fields.iter().enumerate() {
                    div { key: "f{i}", class: "field-box", style: "{css(*r)}" }
                }
                for (i, (cur, r, word)) in view.hits.iter().enumerate() {
                    div { key: "h{i}", class: "{hit_class(*cur, *word, colored)}", style: "{css(*r)}" }
                }
                for (i, r) in view.spell_words.iter().enumerate() {
                    div { key: "sp{i}", class: "spell-word", style: "{css(*r)}" }
                }
                for (i, r) in view.spell_shown.iter().enumerate() {
                    div { key: "spc{i}", class: "spell-current", style: "{css(*r)}" }
                }
                for (i, (class, r)) in cmp_marks.iter().enumerate() {
                    div { key: "c{i}", class: "{class}", style: "{css(*r)}" }
                }
                for (i, (label, calc, r)) in view.field_marks.iter().enumerate() {
                    div { key: "fm{i}", class: "field-mark", style: "{css(*r)}",
                        span { class: if *calc { "num calc" } else { "num" }, "{label}" }
                    }
                }
                for (i, (label, selected, r)) in view.order_boxes.iter().enumerate() {
                    div { key: "ob{i}", class: if *selected { "order-box selected" } else { "order-box" }, style: "{css(*r)}",
                        span { class: "num", "{label}" }
                    }
                }
                for (i, r) in view.images.iter().copied() {
                    div { key: "im{i}", class: if view.image_sel == Some(i) { "image-box selected" } else { "image-box" }, style: "{css(r)}",
                        onmousedown: move |e| {
                            e.stop_propagation();
                            st.with_active(|t| t.image_sel = Some((page, i)));
                        },
                        ondoubleclick: move |e| {
                            e.stop_propagation();
                            st.with_active(|t| t.image_sel = Some((page, i)));
                            crate::commands::pictures::edit_selected(st);
                        },
                    }
                }
                if let Some(r) = view.image_sel.and_then(|i| view.images.iter().find(|(k, _)| *k == i).map(|(_, r)| *r)) {
                    crate::ui::features::pictures::PictureToolbar { left: r[0] * z, top: (r[1] * z - 38.0).max(2.0) }
                }
                if let Some(r) = view.caret {
                    div { class: "text-caret", style: "{css(r)}" }
                }
                for (i, r) in view.sel.iter().enumerate() {
                    div { key: "s{i}", class: "sel", style: "{css(*r)}" }
                }
                if let Some(r) = sel_first {
                    if !*sel_drag.read() {
                        SelToolbar { left: r[0] * z, top: (r[1] * z - 38.0).max(2.0) }
                    }
                }
                for (w, r) in view.widgets.iter().cloned() {
                    {
                        let editing = (st.editing_field)() == Some((page, w.index));
                        let kind = w.kind.clone();
                        let class = match (&kind, w.read_only) {
                            (_, true) => "field-hit readonly",
                            (folitecta_core::FieldKind::Text, _) => "field-hit text",
                            _ => "field-hit",
                        };
                        let w2 = w.clone();
                        rsx! {
                            if editing {
                                FieldInput { key: "fi{w.index}", page, widget: w2, left: r[0] * z, top: r[1] * z, width: (r[2] - r[0]) * z, height: (r[3] - r[1]) * z, zoom: z }
                            } else {
                                div { key: "w{w.index}", class: "{class}", title: "{w.name}", style: "{css(r)}",
                                    onmousedown: move |e| e.stop_propagation(),
                                    onclick: move |e| {
                                        e.stop_propagation();
                                        fill_field(st, page, &w);
                                    },
                                }
                            }
                        }
                    }
                }
                for (index, subtype, rect, contents) in view.annots.iter().cloned() {
                    {
                        let is_sel = view.selected == Some(index);
                        let is_chosen = is_sel || view.chosen.contains(&index);
                        let shown = match cur_drag {
                            Some(d) if d.index == index => dragged_rect(&d, z, view_rotation),
                            // The other chosen comments move with the one dragged.
                            Some(d) if matches!(d.kind, AnnotDragKind::Move) && is_chosen && view.chosen.contains(&d.index) => {
                                let moved = dragged_rect(&d, z, view_rotation);
                                let (dx, dy) = (moved[0] - d.rect[0], moved[1] - d.rect[1]);
                                [rect[0] + dx, rect[1] + dy, rect[2] + dx, rect[3] + dy]
                            }
                            _ => rect,
                        };
                        let title = format!("{}: {contents}", crate::i18n::annot_name(st.lang(), &subtype));
                        let class = match (is_sel, is_chosen) {
                            (true, _) => "annot-hit selected",
                            (false, true) => "annot-hit selected also",
                            _ => "annot-hit",
                        };
                        let can_edit = tool == Tool::EditAnnots || tool == Tool::EditFields;
                        let is_widget = subtype == "Widget";
                        rsx! {
                            div { class: "{class}", key: "a{index}", title: "{title}", style: "{css(shown)}",
                                onmousedown: move |e| {
                                    e.stop_propagation();
                                    // Shift or Ctrl adds the comment to those chosen (or takes it out);
                                    // a comment of a group comes with the whole group. A press on
                                    // a comment already chosen keeps the others, to drag them all.
                                    let more = e.modifiers().shift() || e.modifiers().ctrl();
                                    st.with_active(|t| {
                                        let group = t.doc.annotation_group(page, index);
                                        let mut list = t.chosen_annots().filter(|(p, _)| *p == page).map(|(_, l)| l).unwrap_or_default();
                                        if more {
                                            if list.contains(&index) {
                                                list.retain(|i| !group.contains(i));
                                            } else {
                                                list.extend(group.iter().filter(|g| !list.contains(g)).copied().collect::<Vec<_>>());
                                            }
                                            t.set_chosen(page, &list);
                                        } else if !list.contains(&index) {
                                            t.set_chosen(page, &group);
                                        }
                                        t.text_sel = None;
                                    });
                                    if can_edit && !more && e.trigger_button() == Some(MouseButton::Primary) {
                                        let c = e.client_coordinates();
                                        annot_drag.set(Some(AnnotDrag { index, kind: AnnotDragKind::Move, start_client: (c.x, c.y), cur_client: (c.x, c.y), rect }));
                                    }
                                },
                                ondoubleclick: move |e| {
                                    e.stop_propagation();
                                    if is_widget {
                                        st.ui_mut().show_right_pane = true;
                                        return;
                                    }
                                    // Tables open in the table editor.
                                    if crate::commands::tables::edit_table(st, page, index) {
                                        return;
                                    }
                                    // Calculator tapes open in the calculator pane.
                                    if crate::commands::calculator::edit_calculator(st, page, index) {
                                        return;
                                    }
                                    // Sound and video comments play.
                                    if crate::commands::media::play_item(st, page, index) {
                                        return;
                                    }
                                    let contents = contents.clone();
                                    st.set_dialog(Dialog::AnnotationText { page, index, contents });
                                },
                                if is_sel && can_edit {
                                    for corner in 0u8..4 {
                                        div { key: "c{corner}", class: "handle h{corner}",
                                            onmousedown: move |e| {
                                                e.stop_propagation();
                                                let c = e.client_coordinates();
                                                annot_drag.set(Some(AnnotDrag { index, kind: AnnotDragKind::Resize(corner), start_client: (c.x, c.y), cur_client: (c.x, c.y), rect }));
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(r) = view.flash {
                    div { class: "dest-flash", style: "{css(r)}" }
                }
                for pv in view.popups.iter().cloned() {
                    PopupNote { key: "pn{pv.index}", page, view: pv, zoom: z, size, view_rotation, area_w: inner_w, area_h: inner_h }
                }
                if let Some((pts, closed)) = multi_preview.clone() {
                    svg { class: "ink-preview", width: "{inner_w}", height: "{inner_h}",
                        if closed {
                            polygon { points: "{pts}", fill: "rgba(79,70,229,0.08)", stroke: "#4f46e5", stroke_width: "1.5", stroke_dasharray: "5 3" }
                        } else {
                            polyline { points: "{pts}", fill: "none", stroke: "#4f46e5", stroke_width: "1.5" }
                        }
                        for (i, (vx, vy)) in multi_vertices.iter().enumerate() {
                            circle { key: "v{i}", cx: "{vx}", cy: "{vy}", r: "3.5", fill: "#ffffff", stroke: "#4f46e5", stroke_width: "1.5" }
                        }
                    }
                }
                if let Some(d) = drag() {
                    if matches!(tool, Tool::Ink | Tool::FreeHighlight | Tool::Eraser) {
                        svg { class: "ink-preview", width: "{inner_w}", height: "{inner_h}",
                            match tool {
                                Tool::Eraser => rsx! { polyline { points: "{ink_points}", fill: "none", stroke: "rgba(220,38,38,0.45)", stroke_width: "12", stroke_linecap: "round", stroke_linejoin: "round" } },
                                Tool::FreeHighlight => rsx! { polyline { points: "{ink_points}", fill: "none", stroke: "rgba(250,204,21,0.5)", stroke_width: "{12.0 * z}", stroke_linecap: "round", stroke_linejoin: "round" } },
                                _ => {
                                    let s = crate::state::comment_styles::default_style(st, "Ink");
                                    let color = css_color(s.color.unwrap_or([0.23, 0.23, 0.75]));
                                    let width = (s.width * z).max(1.0);
                                    let opacity = s.opacity.clamp(0.05, 1.0);
                                    rsx! { path { class: "ink-stroke", d: "{ink_curve}", fill: "none", stroke: "{color}", stroke_width: "{width}", opacity: "{opacity}", stroke_linecap: "round", stroke_linejoin: "round" } }
                                }
                            }
                        }
                    } else if let Some(preview) = shape_preview(st, tool, d, z) {
                        svg { class: "ink-preview", width: "{inner_w}", height: "{inner_h}", {preview} }
                    } else {
                        div { class: if tool == Tool::Redact { "marquee redact" } else { "marquee" },
                            style: "left:{d.start.0.min(d.current.0)}px;top:{d.start.1.min(d.current.1)}px;width:{(d.start.0 - d.current.0).abs()}px;height:{(d.start.1 - d.current.1).abs()}px" }
                    }
                }
    };
    // The mouse moves are listened to only with the tools that follow them:
    // a listener of mouse moves makes every move in the window wait for the
    // program, so with the hand and zoom tools (looking at the pages) there
    // is none unless the pointer position is shown. Changing between the two
    // draws the page area again.
    let page_inner = if follows_moves {
        rsx! {
            div { class: inner_class, style: "{inner_style}",
                onmousedown: onmousedown, onmouseup: onmouseup, onmouseleave: onmouseleave, ondoubleclick: ondoubleclick, onclick: onclick, onmousemove: on_move,
                {inner_content}
            }
        }
    } else {
        rsx! {
            div { class: inner_class, style: "{inner_style}",
                onmousedown: onmousedown, onmouseup: onmouseup, onmouseleave: onmouseleave, ondoubleclick: ondoubleclick, onclick: onclick,
                {inner_content}
            }
        }
    };
    rsx! {
        div {
            id: "pg-{page}",
            class: "{page_class}",
            "data-page": "{page}",
            style: "width:{outer_w}px;height:{outer_h}px",
            if rulers {
                Rulers { width: outer_w, height: outer_h, zoom: z, unit_pt, minor, major, unit_name }
            }
            {page_inner}
            div { class: "page-label", "{page_label}" }
            if show_size {
                div { class: "page-size", "{size_text}" }
            }
        }
    }
}
