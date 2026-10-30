//! Creating what the drawing tools draw: form fields, finished multi-click shapes and what happens after.

use super::*;

/// Create a form field from a drag (display rectangle `d`), select it and show its properties.
pub(super) fn create_field(st: AppState, page: usize, size: &PageSize, tool: Tool, d: [f32; 4], tiny: bool) {
    use folitecta_core::{FieldProps, NewField};
    let Some(kind) = tool.new_field() else { return };
    let (dw, dh) = match kind {
        NewField::Text | NewField::ComboBox => (160.0, 22.0),
        NewField::Date => (110.0, 22.0),
        NewField::CheckBox | NewField::RadioButton => (14.0, 14.0),
        NewField::ListBox => (160.0, 64.0),
        NewField::PushButton => (90.0, 24.0),
        NewField::Signature => (180.0, 48.0),
    };
    let rect = if tiny {
        rect_to_pdf(size, [d[0], d[1], d[0] + dw, d[1] + dh])
    } else {
        rect_to_pdf(size, d)
    };
    let mut props = FieldProps::default();
    match kind {
        NewField::RadioButton => {
            // Join the group of the selected radio button, else the group of this tool session.
            let selected_group = st
                .read_active(|t| {
                    let (p, i) = t.selected_annot?;
                    let wid = t
                        .doc
                        .annotations(p)
                        .ok()?
                        .into_iter()
                        .find(|a| a.index == i)?
                        .id?;
                    t.doc
                        .form_fields()
                        .into_iter()
                        .find(|f| {
                            f.kind == folitecta_core::FieldKind::RadioButton
                                && t.doc.field_widgets(f.id).contains(&wid)
                        })
                        .map(|f| f.name)
                })
                .flatten();
            let group = selected_group.or_else(|| st.radio_group.peek().clone());
            let existing = group.as_ref().and_then(|g| {
                st.read_active(|t| {
                    t.doc
                        .form_fields()
                        .into_iter()
                        .find(|f| &f.name == g)
                        .map(|f| t.doc.field_widgets(f.id).len())
                })
                .flatten()
            });
            match (group, existing) {
                (Some(g), Some(n)) => {
                    props.name = g;
                    props.export_value = format!("Choice{}", n + 1);
                }
                _ => props.export_value = "Choice1".into(),
            }
        }
        NewField::ComboBox | NewField::ListBox => {
            props.options = (1..=3)
                .map(|i| format!("{} {i}", t(st.lang(), "field.option")))
                .collect()
        }
        NewField::PushButton => props.caption = t(st.lang(), "field.button_caption"),
        // The date format of the language of the interface.
        NewField::Date => {
            props.format = folitecta_core::FieldFormat::Date(t(st.lang(), "field.date_format_default"))
        }
        _ => {}
    }
    match st.with_active(|t| t.doc.add_form_field(page, kind, rect, &props)) {
        Some(Ok(widget)) => {
            let info = st
                .read_active(|t| {
                    let idx = t
                        .doc
                        .annotations(page)
                        .ok()?
                        .into_iter()
                        .find(|a| a.id == Some(widget))
                        .map(|a| a.index)?;
                    let name = t
                        .doc
                        .form_fields()
                        .into_iter()
                        .find(|f| t.doc.field_widgets(f.id).contains(&widget))
                        .map(|f| f.name)?;
                    Some((idx, name))
                })
                .flatten();
            if let Some((idx, name)) = info {
                if kind == NewField::RadioButton {
                    st.set_radio_group(Some(name));
                }
                st.with_active(|t| t.selected_annot = Some((page, idx)));
            }
            files::set_window_title(st);
            if !option_on(st, "tool.keepSelected") {
                st.set_tool(Tool::EditFields);
            }
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

/// Finish the shape drawn with a multi-click tool (polygon, polyline, cloud,
/// perimeter, area). Called on double click and on Enter.
pub fn finish_drawing(st: AppState) {
    let Some((page, pts)) = st.drawing.peek().clone() else {
        return;
    };
    st.set_drawing(None);
    let tool = *st.tool.peek();
    let Some(size) = st.read_active(|t| t.page_sizes.get(page).copied()).flatten() else {
        return;
    };
    // A double click adds the same point twice.
    let mut pts = pts;
    pts.dedup_by(|a, b| (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5);
    let pdf: Vec<(f32, f32)> = pts.iter().map(|(x, y)| to_pdf(&size, *x, *y)).collect();
    let author = st.ui.peek().author.clone();
    let style = |kind: &str| crate::state::comment_styles::default_style(st, kind);
    let scale = crate::ui::dialogs::measure_scale(st);
    let r = st.with_active(|t| match tool {
        Tool::Polygon => t
            .doc
            .add_polyline(page, &pdf, true, &style("Polygon"), &author, "")
            .map(|_| ()),
        Tool::Polyline => t
            .doc
            .add_polyline(page, &pdf, false, &style("PolyLine"), &author, "")
            .map(|_| ()),
        Tool::Cloud => t.doc.add_cloud(page, &pdf, &style("Cloud"), &author).map(|_| ()),
        Tool::Perimeter => t
            .doc
            .add_measurement(
                page,
                folitecta_core::MeasureKind::Perimeter,
                &pdf,
                &style("Perimeter"),
                &scale,
                &author,
            )
            .map(|_| ()),
        Tool::Area => t
            .doc
            .add_measurement(
                page,
                folitecta_core::MeasureKind::Area,
                &pdf,
                &style("Area"),
                &scale,
                &author,
            )
            .map(|_| ()),
        Tool::Angle if pdf.len() >= 3 => t
            .doc
            .add_angle_measure(page, [pdf[0], pdf[1], pdf[2]], &style("Angle"), &scale, &author)
            .map(|_| ()),
        // Its depth is asked first.
        Tool::Volume if pdf.len() >= 3 => {
            st.set_dialog(Dialog::VolumeDepth {
                page,
                points: pdf.clone(),
            });
            Ok(())
        }
        _ => Ok(()),
    });
    match r {
        Some(Err(e)) => st.error(e),
        Some(Ok(())) => after_create(st),
        None => {}
    }
}

/// The tools that stay chosen after what they draw.
pub fn stays_chosen(tool: Tool) -> bool {
    crate::ui::features::quick_panel::measuring(tool)
        || matches!(
            tool,
            Tool::Ink
                | Tool::FreeHighlight
                | Tool::Tick
                | Tool::Rect
                | Tool::Ellipse
                | Tool::Line
                | Tool::Arrow
                | Tool::Polygon
                | Tool::Polyline
                | Tool::Cloud
        )
}

pub fn after_create(st: AppState) {
    files::set_window_title(st);
    // A freehand drawing takes many strokes, tick marks go one after the
    // other, shapes and measures are drawn several at a time: the pencil,
    // the free highlighter, the tick marks, the shapes and the measuring
    // tools stay chosen (Escape goes back to the hand).
    if !stays_chosen(*st.tool.peek()) && !option_on(st, "tool.keepSelected") {
        st.set_tool(Tool::Hand);
    }
}

/// What a drag (or a click) with a drawing tool makes on page `page`: `d` in
/// pixels of the page at zoom `z`, `points` the path of the freehand tools,
/// `pressures` how hard a pen pressed at each of them (empty for a mouse).
#[allow(clippy::too_many_arguments)]
pub(super) fn finish_create(
    st: AppState,
    page: usize,
    size: PageSize,
    tool: Tool,
    z: f32,
    d: Drag,
    points: Vec<(f64, f64)>,
    pressures: Vec<f32>,
) {
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
    let style = |kind: &str| crate::state::comment_styles::default_style(st, kind);
    let path_pdf = || -> Vec<(f32, f32)> {
        points
            .iter()
            .map(|(x, y)| to_pdf(&size, *x as f32 / z, *y as f32 / z))
            .collect()
    };
    let result: Option<folitecta_core::Result<()>> = match tool {
        // A tick mark where the click was.
        Tool::Tick => Some(crate::commands::ticks::place(st, page, point)),
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
        Tool::Radius if !tiny => {
            let scale = crate::ui::dialogs::measure_scale(st);
            st.with_active(|t| {
                t.doc
                    .add_radius_measure(page, start_pdf, point, &style("Radius"), &scale, &author)
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
        // A stroke of a pen that told how hard it pressed, its width following.
        Tool::Ink
            if points.len() > 1
                && pressures.len() == points.len()
                && crate::ui::features::quick_panel::pressure_on(st) =>
        {
            let (path, pressed) =
                folitecta_core::smooth::smooth_pressed_stroke(&path_pdf(), &pressures, 0.5 / z, 2.0);
            st.with_active(|t| {
                t.doc
                    .add_pressed_ink(page, &path, &pressed, &style("Ink"), &author)
                    .map(|_| ())
            })
        }
        Tool::Ink | Tool::FreeHighlight if points.len() > 1 => {
            let s = style(if tool == Tool::Ink { "Ink" } else { "FreeHighlight" });
            // Pointer jitter under half a screen pixel dropped, a piece of
            // curve at least every two points of length.
            let path = folitecta_core::smooth::smooth_stroke(&path_pdf(), 0.5 / z, 2.0);
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
        Tool::Redact if !tiny => {
            let look = crate::commands::redaction::default_look(st);
            st.with_active(|t| t.doc.add_redaction_with(page, rect, &author, &look).map(|_| ()))
        }
        Tool::Table if !tiny => {
            st.set_dialog(Dialog::TableNew { page, rect });
            None
        }
        Tool::Barcode => {
            // A click places a barcode of 100 points around it.
            let r = if tiny {
                [x0 - 50.0, y0 - 50.0, x0 + 50.0, y0 + 50.0]
            } else {
                [x0, y0, x1, y1]
            };
            st.set_dialog(Dialog::Barcode { page, rect: r });
            None
        }
        Tool::BarcodeScan if !tiny => {
            crate::commands::barcodes::read_area(st, page, [x0, y0, x1, y1]);
            None
        }
        Tool::OcrRegion if !tiny => {
            crate::commands::ocr::read_area(st, page, [x0, y0, x1, y1]);
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
            // The area as the view shows it.
            let png = st.read_active(|t| {
                t.doc
                    .render_region_png_with_view(page, [x0, y0, x1, y1], 2.0, &t.view_layer_states())
            });
            // "New stamp from selection" keeps the area as a stamp.
            let to_stamp = st.ui.read().value("snapshot.target") == Some("stamp");
            match png {
                Some(Ok(png)) if to_stamp => {
                    st.set_value("snapshot.target", "");
                    let name = t(st.lang(), "stamps.from_area");
                    crate::commands::doctools::picture_stamp(st, png, name);
                    st.set_tool(Tool::Hand);
                }
                Some(Ok(_)) if !st.allowed(folitecta_core::Restricted::Copy) => {}
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
}
