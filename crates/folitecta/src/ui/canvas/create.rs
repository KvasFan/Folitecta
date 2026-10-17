//! Creating what the drawing tools draw: form fields, finished multi-click shapes and what happens after.

use super::*;

/// Create a form field from a drag (display rectangle `d`), select it and show its properties.
pub(super) fn create_field(st: AppState, page: usize, size: &PageSize, tool: Tool, d: [f32; 4], tiny: bool) {
    use folitecta_core::{FieldProps, NewField};
    let Some(kind) = tool.new_field() else { return };
    let (dw, dh) = match kind {
        NewField::Text | NewField::ComboBox => (160.0, 22.0),
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
    let style = |kind: &str| crate::styles::default_style(st, kind);
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
        _ => Ok(()),
    });
    match r {
        Some(Err(e)) => st.error(e),
        Some(Ok(())) => after_create(st),
        None => {}
    }
}

pub(super) fn after_create(st: AppState) {
    files::set_window_title(st);
    if !option_on(st, "tool.keepSelected") {
        st.set_tool(Tool::Hand);
    }
}
