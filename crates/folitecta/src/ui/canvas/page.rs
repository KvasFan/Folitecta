//! One page of the canvas: its picture, comments, links, fields, search results, selections and the tools at work on it.

use super::*;

/// Class of a search hit: the current one, and one color per word of the query.
pub(super) fn hit_class(current: bool, word: usize, colored: bool) -> String {
    let mut c = String::from("hit");
    if current {
        c.push_str(" current");
    }
    if colored {
        c.push_str(&format!(" w{}", word % 5));
    }
    c
}

#[component]
pub(super) fn PageView(
    tab_id: u64,
    page: usize,
    rev: u64,
    scale: u32,
    zoom: f32,
    current: bool,
    tool: Tool,
    view_rotation: i32,
    label: String,
) -> Element {
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
        guides: Vec<(bool, f32)>,
        selected: Option<usize>,
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
    }
    let order_mode = {
        let ui = st.ui.read();
        (ui.show_left_pane && ui.left_pane == "order").then(|| crate::commands::accessibility::order_mode(st))
    }
    .filter(|m| m != "none");
    let order_sel = (st.current_page() == page)
        .then(|| st.read_active(|t| t.acc.order).flatten())
        .flatten();
    let order_cells = st.value("order.cells") == "1";
    let order_only = st.value("order.tf") == "1";
    let show_tabs = option_on(st, "tool.editFields.showTabNumbers");
    let show_calc = option_on(st, "tool.editFields.showCalcOrdIdx");
    let designing = edit_fields || tool.new_field().is_some();
    let Some(view) = st
        .read_active(|t| {
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
                        .text_layers()
                        .get(page)
                        .map(|l| l.range_rects(a, b))
                        .unwrap_or_default()
                }
                _ => Vec::new(),
            };
            let guides = t
                .guides
                .iter()
                .filter(|g| g.page.is_none() || g.page == Some(page))
                .map(|g| (g.horizontal, g.pos))
                .collect();
            let selected = t.selected_annot.filter(|(p, _)| *p == page).map(|(_, i)| i);
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
            Some(View {
                size,
                annots,
                links,
                fields,
                hits,
                sel,
                guides,
                selected,
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
    let src = if transparent {
        format!("/page/{tab_id}/{page}/{rev}/{scale}.png?bg=0")
    } else {
        format!("/page/{tab_id}/{page}/{rev}/{scale}.png")
    };
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
    let finish_create = move |d: Drag, points: Vec<(f64, f64)>| {
        let (x0, y0) = (
            d.start.0.min(d.current.0) as f32 / z,
            d.start.1.min(d.current.1) as f32 / z,
        );
        let (x1, y1) = (
            d.start.0.max(d.current.0) as f32 / z,
            d.start.1.max(d.current.1) as f32 / z,
        );
        let tiny = (x1 - x0) < 3.0 && (y1 - y0) < 3.0;
        let author = st.ui.read().author.clone();
        let rect = rect_to_pdf(&size, [x0, y0, x1, y1]);
        let start_pdf = to_pdf(&size, d.start.0 as f32 / z, d.start.1 as f32 / z);
        let point = to_pdf(&size, d.current.0 as f32 / z, d.current.1 as f32 / z);
        let style = |kind: &str| crate::styles::default_style(st, kind);
        let path_pdf = || -> Vec<(f32, f32)> {
            points
                .iter()
                .map(|(x, y)| to_pdf(&size, *x as f32 / z, *y as f32 / z))
                .collect()
        };
        let result: Option<folitecta_core::Result<()>> = match tool {
            Tool::Note => {
                st.set_dialog(Dialog::Note {
                    page,
                    x: point.0,
                    y: point.1,
                });
                None
            }
            Tool::TextBox | Tool::Typewriter => {
                let r = if tiny {
                    rect_to_pdf(&size, [x0, y0, x0 + 220.0, y0 + 40.0])
                } else {
                    rect
                };
                st.set_dialog(Dialog::TextBox {
                    page,
                    rect: r,
                    border: tool == Tool::TextBox,
                });
                None
            }
            Tool::Callout => {
                // The press is the pointed spot, the release places the text box.
                let (bx, by) = (d.current.0 as f32 / z, d.current.1 as f32 / z);
                let (bx, by) = if tiny { (bx + 40.0, by - 60.0) } else { (bx, by) };
                let r = rect_to_pdf(&size, [bx, by - 20.0, bx + 170.0, by + 30.0]);
                st.set_dialog(Dialog::Callout {
                    page,
                    tip: start_pdf,
                    rect: r,
                });
                None
            }
            Tool::Link if !tiny => {
                st.set_dialog(Dialog::Link { page, rect });
                None
            }
            Tool::SignArea if !tiny => {
                crate::commands::signatures::finish_sign_area(st, page, rect);
                None
            }
            Tool::PlaceSignature => {
                crate::commands::signs::place_at(st, page, rect, tiny);
                None
            }
            Tool::Rect if !tiny => st.with_active(|t| {
                t.doc
                    .add_shape_styled(page, Shape::Square, rect, &style("Square"), &author, "")
                    .map(|_| ())
            }),
            Tool::Ellipse if !tiny => st.with_active(|t| {
                t.doc
                    .add_shape_styled(page, Shape::Circle, rect, &style("Circle"), &author, "")
                    .map(|_| ())
            }),
            Tool::Highlight if !tiny => {
                let q = [
                    rect[0], rect[3], rect[2], rect[3], rect[0], rect[1], rect[2], rect[1],
                ];
                let c = style("HighlightArea").color.unwrap_or([1.0, 0.9, 0.2]);
                st.with_active(|t| {
                    t.doc
                        .add_text_markup(page, Markup::Highlight, &[q], c, &author, "")
                        .map(|_| ())
                })
            }
            Tool::Line | Tool::Arrow if !tiny => {
                let mut s = style("Line");
                if tool == Tool::Arrow && s.line_end == "None" {
                    s.line_end = "OpenArrow".into();
                }
                st.with_active(|t| {
                    t.doc
                        .add_polyline(page, &[start_pdf, point], false, &s, &author, "")
                        .map(|_| ())
                })
            }
            Tool::Distance if !tiny => {
                let scale = crate::ui::dialogs::measure_scale(st);
                st.with_active(|t| {
                    t.doc
                        .add_measurement(
                            page,
                            folitecta_core::MeasureKind::Distance,
                            &[start_pdf, point],
                            &style("Distance"),
                            &scale,
                            &author,
                        )
                        .map(|_| ())
                })
            }
            Tool::Ink | Tool::FreeHighlight if points.len() > 1 => {
                let s = style(if tool == Tool::Ink { "Ink" } else { "FreeHighlight" });
                let path = path_pdf();
                st.with_active(|t| t.doc.add_ink_styled(page, &[path], &s, &author).map(|_| ()))
            }
            Tool::Eraser => {
                let path = path_pdf();
                // Radius of 6 screen pixels, in page points.
                let radius = (6.0 / z).max(1.5);
                match st.with_active(|t| t.doc.erase_ink(page, &path, radius)) {
                    Some(Ok(_)) => files::set_window_title(st),
                    Some(Err(e)) => st.error(e),
                    None => {}
                }
                None
            }
            Tool::FileAttachment => {
                files::pick_page_attachment(st, page, point);
                None
            }
            Tool::Redact if !tiny => st.with_active(|t| t.doc.add_redaction(page, rect, &author).map(|_| ())),
            Tool::Table if !tiny => {
                st.set_dialog(Dialog::TableNew { page, rect });
                None
            }
            Tool::Calculator => {
                let top_left = (rect[0], rect[3]);
                crate::commands::calculator::place(st, page, top_left);
                None
            }
            Tool::Sound => {
                crate::commands::media::pick_sound(st, page, (rect[0], rect[3]));
                None
            }
            Tool::Video => {
                crate::commands::media::pick_media(st, page, rect);
                None
            }
            Tool::Stamp if st.ui.read().value("stamp.kind") == Some("design") => {
                let c = rect_to_pdf(&size, [x0, y0, x0, y0]);
                crate::commands::stamps::place_design(st, page, rect, tiny, (c[0], c[1]))
            }
            Tool::Stamp => {
                if let Some(p) = crate::commands::stamps::pending() {
                    crate::commands::stamps::placed(p.item.id);
                }
                let image = if st.ui.read().value("stamp.kind") == Some("image") {
                    st.pending_image.peek().clone()
                } else {
                    None
                };
                match image {
                    Some(data) => {
                        let r = if tiny {
                            let (iw, ih) = folitecta_core::Document::image_size(&data)
                                .map(|(w, h)| (w as f32, h as f32))
                                .unwrap_or((160.0, 80.0));
                            let s = (160.0 / iw.max(ih)).min(1.0);
                            let (w, h) = ((iw * s).max(8.0), (ih * s).max(8.0));
                            rect_to_pdf(&size, [x0 - w / 2.0, y0 - h / 2.0, x0 + w / 2.0, y0 + h / 2.0])
                        } else {
                            rect
                        };
                        st.with_active(|t| t.doc.add_image_stamp(page, r, data, &author, "").map(|_| ()))
                    }
                    None => {
                        let text = st.ui.read().value("stamp.text").unwrap_or("APPROVED").to_string();
                        let color =
                            crate::util::parse_color(st.ui.read().value("stamp.color").unwrap_or("#c81e1e"));
                        let r = if tiny {
                            let w = (folitecta_core::util::helvetica_width(&text, 20.0) + 30.0).max(90.0);
                            rect_to_pdf(&size, [x0 - w / 2.0, y0 - 22.0, x0 + w / 2.0, y0 + 22.0])
                        } else {
                            rect
                        };
                        st.with_active(|t| t.doc.add_stamp(page, r, &text, color, &author).map(|_| ()))
                    }
                }
            }
            Tool::Snapshot if !tiny => {
                let png = st.read_active(|t| t.doc.render_region_png(page, [x0, y0, x1, y1], 2.0));
                // "New stamp from selection" keeps the area as a stamp.
                let to_stamp = st.ui.read().value("snapshot.target") == Some("stamp");
                match png {
                    Some(Ok(png)) if to_stamp => {
                        st.set_value("snapshot.target", "");
                        let name = t(st.lang(), "stamps.from_area");
                        crate::commands::doctools::picture_stamp(st, png, name);
                        st.set_tool(Tool::Hand);
                    }
                    Some(Ok(png)) => match crate::clipboard::set_png(&png) {
                        Ok(()) => st.toast(ToastKind::Ok, t(st.lang(), "toast.snapshot_copied"), ""),
                        Err(e) => st.error(e),
                    },
                    Some(Err(e)) => st.error(e),
                    None => {}
                }
                None
            }
            _ if tool.new_field().is_some() => {
                create_field(st, page, &size, tool, [x0, y0, x1, y1], tiny);
                None
            }
            Tool::PlaceImage => {
                let data = st.pending_image.peek().clone();
                match data {
                    Some(data) => {
                        // add_image takes display space with y up.
                        let display = if tiny {
                            None
                        } else {
                            Some([x0, size.height - y1, x1, size.height - y0])
                        };
                        let r = st.with_active(|t| t.doc.add_image(page, data, display).map(|_| ()));
                        st.set_pending_image(None);
                        r
                    }
                    None => None,
                }
            }
            _ => None,
        };
        match result {
            Some(Err(e)) => st.error(e),
            Some(Ok(())) => after_create(st),
            None => {}
        }
    };

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
        if tool == Tool::ReplaceText {
            if let Some(sel) = st.read_active(|t| t.text_sel).flatten().filter(|s| !s.is_empty()) {
                let (start, end) = sel.range();
                st.set_dialog(Dialog::ReplaceText {
                    page: sel.page,
                    start,
                    end,
                });
            }
        }
    };

    let page_class = if current { "page current-page" } else { "page" };
    let ink_points: String = ink()
        .iter()
        .map(|(x, y)| format!("{x},{y}"))
        .collect::<Vec<_>>()
        .join(" ");
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

    rsx! {
        div {
            id: "pg-{page}",
            class: "{page_class}",
            "data-page": "{page}",
            style: "width:{outer_w}px;height:{outer_h}px",
            if rulers {
                Rulers { width: outer_w, height: outer_h, zoom: z, unit_pt, minor, major, unit_name }
            }
            div {
                class: if transparent { "page-inner transparent" } else { "page-inner" },
                style: "width:{inner_w}px;height:{inner_h}px;transform:{transform}",
                onmousedown: move |evt| {
                    if evt.trigger_button() != Some(MouseButton::Primary) {
                        return;
                    }
                    let p = evt.element_coordinates();
                    let (x, y) = ((p.x as f32) / z, (p.y as f32) / z);
                    if !current {
                        st.with_active(|t| t.page = page);
                    }
                    if tool == Tool::ZoomDyn {
                        let c = evt.client_coordinates();
                        *crate::commands::fileops::ZOOM_DRAG.write() = Some((c.y, current_zoom(st)));
                        return;
                    }
                    if tool.is_text() {
                        let idx = st.read_active(|t| t.doc.text_layers().get(page).and_then(|l| l.char_at(x, y))).flatten();
                        match idx {
                            Some(i) => {
                                let extend = evt.modifiers().shift();
                                st.with_active(|t| {
                                    t.selected_annot = None;
                                    match (extend, t.text_sel) {
                                        (true, Some(s)) if s.page == page => t.text_sel = Some(TextSel { head: i, ..s }),
                                        _ => t.text_sel = Some(TextSel { page, anchor: i, head: i }),
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
                        let mut pts = st.drawing.peek().clone().filter(|(p, _)| *p == page).map(|(_, v)| v).unwrap_or_default();
                        pts.push((sx, sy));
                        st.set_drawing(Some((page, pts)));
                        return;
                    }
                    if tool == Tool::InsertText {
                        let hit = st
                            .read_active(|t| {
                                let layers = t.doc.text_layers();
                                let l = layers.get(page)?;
                                let i = l.char_at(x, y)?;
                                let after = l.char_glyph.get(i).copied().flatten().and_then(|g| l.glyphs.get(g)).map(|g| x > (g.bbox[0] + g.bbox[2]) / 2.0).unwrap_or(false);
                                Some((i, after))
                            })
                            .flatten();
                        match hit {
                            Some((index, after)) => st.set_dialog(Dialog::InsertText { page, index, after }),
                            None => st.toast(ToastKind::Info, t(st.lang(), "toast.click_on_text"), ""),
                        }
                        return;
                    }
                    if tool == Tool::EditAnnots || tool == Tool::Hand {
                        st.with_active(|t| t.selected_annot = None);
                        return;
                    }
                    if tool == Tool::EditImages {
                        st.with_active(|t| t.image_sel = None);
                        return;
                    }
                    if tool.is_drawing() {
                        let (sx, sy) = snap(st, page, x, y, z);
                        let start = ((sx * z) as f64, (sy * z) as f64);
                        drag.set(Some(Drag { start, current: start }));
                        ink.set(vec![start]);
                    }
                },
                onmousemove: move |evt| {
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
                    if tool == Tool::Loupe && (option_on(st, "tool.loupe.trackMouse") || evt.held_buttons().contains(dioxus::html::input_data::MouseButton::Primary)) {
                        let c = evt.client_coordinates();
                        st.set_loupe(Some((page, x, y, c.x, c.y)));
                    }
                    if *sel_drag.peek() {
                        if let Some(i) = st.read_active(|t| t.doc.text_layers().get(page).and_then(|l| l.char_at(x, y))).flatten() {
                            let changed = st.read_active(|t| t.text_sel.map(|s| s.head != i).unwrap_or(true)).unwrap_or(false);
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
                    if tool.is_multi_click() && st.drawing.peek().as_ref().map(|(p, _)| *p == page).unwrap_or(false) {
                        let (sx, sy) = snap(st, page, x, y, z);
                        hover.set(Some((sx, sy)));
                    }
                    let freehand = matches!(tool, Tool::Ink | Tool::FreeHighlight | Tool::Eraser);
                    let current_drag: Option<Drag> = *drag.peek();
                    if let Some(mut d) = current_drag {
                        let (sx, sy) = if freehand { (x, y) } else { snap(st, page, x, y, z) };
                        d.current = ((sx * z) as f64, (sy * z) as f64);
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
                },
                onmouseup: move |_| {
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
                },
                onmouseleave: move |_| {
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
                },
                ondoubleclick: move |evt| {
                    if tool.is_multi_click() {
                        finish_drawing(st);
                        return;
                    }
                    if tool != Tool::Select {
                        return;
                    }
                    let p = evt.element_coordinates();
                    let (x, y) = ((p.x as f32) / z, (p.y as f32) / z);
                    let word = st.read_active(|t| t.doc.text_layers().get(page).and_then(|l| l.char_at(x, y).map(|i| l.word_at(i.saturating_sub(0))))).flatten();
                    if let Some((a, b)) = word {
                        st.with_active(|t| t.text_sel = Some(TextSel { page, anchor: a, head: b }));
                    }
                },
                onclick: move |evt| {
                    if tool == Tool::Zoom {
                        let z = current_zoom(st);
                        let f = if evt.modifiers().shift() { 0.8 } else { 1.25 };
                        st.with_active(|t| t.zoom = Zoom::Fixed((z * f).clamp(0.1, 8.0)));
                    }
                },
                img { class: "page-img", src: "{src}", alt: "", loading: "lazy", draggable: "false", style: if img_filter.is_empty() { String::new() } else { format!("filter:{img_filter}") } }
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
                    crate::ui::pictures::PictureToolbar { left: r[0] * z, top: (r[1] * z - 38.0).max(2.0) }
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
                        let shown = match cur_drag { Some(d) if d.index == index => dragged_rect(&d, z, view_rotation), _ => rect };
                        let title = format!("{}: {contents}", crate::i18n::annot_name(st.lang(), &subtype));
                        let class = if is_sel { "annot-hit selected" } else { "annot-hit" };
                        let can_edit = tool == Tool::EditAnnots || tool == Tool::EditFields;
                        let is_widget = subtype == "Widget";
                        rsx! {
                            div { class: "{class}", key: "a{index}", title: "{title}", style: "{css(shown)}",
                                onmousedown: move |e| {
                                    e.stop_propagation();
                                    st.with_active(|t| { t.selected_annot = Some((page, index)); t.text_sel = None; });
                                    if can_edit && e.trigger_button() == Some(MouseButton::Primary) {
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
                                _ => rsx! { polyline { points: "{ink_points}", fill: "none", stroke: "#3b3bc0", stroke_width: "2" } },
                            }
                        }
                    } else if matches!(tool, Tool::Line | Tool::Arrow | Tool::Distance | Tool::Callout) {
                        svg { class: "ink-preview", width: "{inner_w}", height: "{inner_h}",
                            line { x1: "{d.start.0}", y1: "{d.start.1}", x2: "{d.current.0}", y2: "{d.current.1}", stroke: "#3b3bc0", stroke_width: "2" }
                        }
                    } else {
                        div { class: if tool == Tool::Redact { "marquee redact" } else { "marquee" },
                            style: "left:{d.start.0.min(d.current.0)}px;top:{d.start.1.min(d.current.1)}px;width:{(d.start.0 - d.current.0).abs()}px;height:{(d.start.1 - d.current.1).abs()}px" }
                    }
                }
            }
            div { class: "page-label", "{page_label}" }
            if show_size {
                div { class: "page-size", "{size_text}" }
            }
        }
    }
}
