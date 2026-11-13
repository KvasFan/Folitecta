//! The go-to views and the layer actions of the chosen link's actions
//! (Document › Action › Go to, Document › Layer Actions): a go-to takes the
//! view of the window, is edited, named, reset or zoomed; a layer action
//! turns layers on, off or toggles them, all of them or those chosen; next
//! layer actions are made one. Every change is one undo step.

use dioxus::prelude::ReadableExt;
use folitecta_core::{ActionInfo, DestView, LayerState};

use crate::commands::link_actions::{ACTION_SEL, chosen_link};
use crate::commands::{handlers, no_doc};
use crate::i18n::t;
use crate::state::{AppState, Dialog, ToastKind};

/// The link chosen (page, index), saying so when there is none.
fn need_link(st: AppState) -> Option<(usize, usize)> {
    if no_doc(st) {
        return None;
    }
    let link = chosen_link(st);
    if link.is_none() {
        st.toast(ToastKind::Info, t(st.lang(), "link_actions.choose_link"), "");
    }
    link
}

/// The action chosen in the link's list and what it does.
fn chosen_action(st: AppState) -> Option<(usize, usize, usize, ActionInfo)> {
    let (p, i) = need_link(st)?;
    let at = ACTION_SEL
        .peek()
        .filter(|(q, j, _)| *q == p && *j == i)
        .map(|(_, _, a)| a);
    let Some(at) = at else {
        st.toast(ToastKind::Info, t(st.lang(), "link_actions.choose_action"), "");
        return None;
    };
    let info = st.read_active(|t| t.doc.link_action_info(p, i, at)).flatten()?;
    Some((p, i, at, info))
}

/// The chosen action when it is a go-to: link, place, page and view.
fn chosen_goto(st: AppState) -> Option<(usize, usize, usize, usize, Option<DestView>)> {
    match chosen_action(st)? {
        (
            p,
            i,
            at,
            ActionInfo::GoTo {
                page: Some(target),
                view,
                ..
            },
        ) => Some((p, i, at, target, view)),
        _ => {
            st.toast(ToastKind::Info, t(st.lang(), "toast.choose_goto"), "");
            None
        }
    }
}

fn report(st: AppState, link: (usize, usize), at: Option<usize>, r: Option<folitecta_core::Result<()>>) {
    st.with_active(|t| t.selected_annot = Some(link));
    if let Some(at) = at {
        *ACTION_SEL.write() = Some((link.0, link.1, at));
    }
    match r {
        Some(Ok(())) => crate::files::set_window_title(st),
        Some(Err(e)) => st.error(st.core_error_text(&e)),
        None => {}
    }
}

/// The view of the window: its page, the point at its top left, its zoom.
async fn window_view(st: AppState) -> (usize, DestView) {
    let page = st.current_page();
    let zoom = crate::commands::current_zoom(st);
    let js = crate::ui::canvas::VIEWPORT_JS.replace("PAGE", &page.to_string());
    let shown: Option<[f64; 2]> = dioxus::document::eval(&js).await.ok().and_then(|v| {
        let a = v.as_array()?;
        Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?])
    });
    let crop = st
        .read_active(|t| t.page_sizes.get(page).map(|s| s.crop_box))
        .flatten();
    let (left, top) = match (shown, crop) {
        (Some([fx, fy]), Some(c)) => (
            Some(c[0] + fx as f32 * (c[2] - c[0])),
            Some(c[3] - fy as f32 * (c[3] - c[1])),
        ),
        (_, Some(c)) => (Some(c[0]), Some(c[3])),
        _ => (None, None),
    };
    (
        page,
        DestView::Xyz {
            left,
            top,
            zoom: Some(zoom),
        },
    )
}

fn add_current(st: AppState) {
    let Some((p, i)) = need_link(st) else { return };
    dioxus::prelude::spawn(async move {
        let (target, view) = window_view(st).await;
        let len = st.read_active(|t| t.doc.link_actions(p, i).len()).unwrap_or(0);
        let r = st.with_active(|t| t.doc.set_link_goto(p, i, usize::MAX, target, &view));
        report(st, (p, i), Some(len), r);
    });
}

fn use_current(st: AppState) {
    let Some((p, i, at, _, _)) = chosen_goto(st) else {
        return;
    };
    dioxus::prelude::spawn(async move {
        let (target, view) = window_view(st).await;
        let r = st.with_active(|t| t.doc.set_link_goto(p, i, at, target, &view));
        report(st, (p, i), Some(at), r);
    });
}

fn set_view(st: AppState, f: impl FnOnce(Option<DestView>) -> DestView) {
    let Some((p, i, at, target, view)) = chosen_goto(st) else {
        return;
    };
    let new = f(view);
    let r = st.with_active(|t| t.doc.set_link_goto(p, i, at, target, &new));
    report(st, (p, i), Some(at), r);
}

/// The zoom of the chosen go-to times `by` (from 100 % when it has none).
fn zoom(st: AppState, by: f32) {
    set_view(st, |v| {
        let (left, top, z) = match v {
            Some(DestView::Xyz { left, top, zoom }) => (left, top, zoom),
            _ => (None, None, None),
        };
        DestView::Xyz {
            left,
            top,
            zoom: Some((z.unwrap_or(1.0) * by).clamp(0.1, 64.0)),
        }
    });
}

fn edit(st: AppState) {
    if let Some((page, index, at, _, _)) = chosen_goto(st) {
        st.set_dialog(Dialog::GoToView { page, index, at });
    }
}

fn rename(st: AppState) {
    if let Some((page, index, at, info)) = chosen_action(st) {
        match info {
            ActionInfo::GoTo {
                named, page: Some(_), ..
            } => st.set_dialog(Dialog::GoToName {
                page,
                index,
                at,
                name: named.unwrap_or_default(),
            }),
            _ => st.toast(ToastKind::Info, t(st.lang(), "toast.choose_goto"), ""),
        }
    }
}

/// Give the go-to its view (from the Edit window).
pub fn store_view(st: AppState, page: usize, index: usize, at: usize, target: usize, view: DestView) {
    let r = st.with_active(|t| t.doc.set_link_goto(page, index, at, target, &view));
    report(st, (page, index), Some(at), r);
}

pub fn store_name(st: AppState, page: usize, index: usize, at: usize, name: &str) {
    let r = st.with_active(|t| t.doc.name_link_goto(page, index, at, name));
    report(st, (page, index), Some(at), r);
}

/// Turn layers on, off or toggle them: the chosen layer action changes its
/// word; otherwise a layer action of every layer is added.
fn change_state(st: AppState, state: LayerState) {
    let Some((p, i)) = need_link(st) else { return };
    let at = ACTION_SEL
        .peek()
        .filter(|(q, j, _)| *q == p && *j == i)
        .map(|(_, _, a)| a);
    let info = at.and_then(|a| st.read_active(|t| t.doc.link_action_info(p, i, a)).flatten());
    let len = st.read_active(|t| t.doc.link_actions(p, i).len()).unwrap_or(0);
    let (at, layers) = match (at, info) {
        (Some(a), Some(ActionInfo::Layers { layers, .. })) => (a, Some(layers)),
        _ => (len, None),
    };
    let r = st.with_active(|t| t.doc.set_link_layer_action(p, i, at, state, layers.as_deref()));
    report(st, (p, i), Some(at), r);
}

fn state_of(word: &str) -> LayerState {
    match word {
        "ON" => LayerState::On,
        "OFF" => LayerState::Off,
        _ => LayerState::Toggle,
    }
}

/// A layer action of a link: page, link, place, word and layers.
type LayerAction = (usize, usize, usize, String, Vec<(u32, u16)>);

/// The chosen layer action.
fn chosen_layers(st: AppState) -> Option<LayerAction> {
    match chosen_action(st)? {
        (p, i, at, ActionInfo::Layers { state, layers }) => Some((p, i, at, state, layers)),
        _ => {
            st.toast(ToastKind::Info, t(st.lang(), "toast.choose_layer_action"), "");
            None
        }
    }
}

fn all_layers(st: AppState) {
    if let Some((p, i, at, state, _)) = chosen_layers(st) {
        let r = st.with_active(|t| t.doc.set_link_layer_action(p, i, at, state_of(&state), None));
        report(st, (p, i), Some(at), r);
    }
}

fn some_layers(st: AppState) {
    if let Some((page, index, at, _, _)) = chosen_layers(st) {
        st.set_dialog(Dialog::LayerActionLayers { page, index, at });
    }
}

/// The layers of the chosen layer action (from the Layers window).
pub fn store_layers(st: AppState, page: usize, index: usize, at: usize, layers: Vec<(u32, u16)>) {
    let word = match st
        .read_active(|t| t.doc.link_action_info(page, index, at))
        .flatten()
    {
        Some(ActionInfo::Layers { state, .. }) => state,
        _ => "Toggle".into(),
    };
    let r = st.with_active(|t| {
        t.doc
            .set_link_layer_action(page, index, at, state_of(&word), Some(&layers))
    });
    report(st, (page, index), Some(at), r);
}

fn simplify(st: AppState) {
    let Some((p, i)) = need_link(st) else { return };
    match st.with_active(|t| t.doc.simplify_link_actions(p, i)) {
        Some(Ok(0)) => st.toast(ToastKind::Info, t(st.lang(), "toast.actions_simple"), ""),
        Some(Ok(n)) => {
            *ACTION_SEL.write() = None;
            st.with_active(|t| t.selected_annot = Some((p, i)));
            crate::files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                crate::i18n::tf(
                    st.lang(),
                    "toast.actions_simplified",
                    &[("count", &n.to_string())],
                ),
                "",
            );
        }
        Some(Err(e)) => st.error(st.core_error_text(&e)),
        None => {}
    }
}

handlers! {
    "action.goto.addCurrentView" => add_current,
    "action.goto.useCurrentView" => use_current,
    "action.goto.edit" => edit,
    "action.goto.rename" => rename,
    "action.goto.delete" => |st| crate::commands::dispatch(st, "actions.remove"),
    "action.goto.reset" => |st| set_view(st, |_| DestView::Fit),
    "action.goto.zoomIn" => |st| zoom(st, 1.25),
    "action.goto.zoomOut" => |st| zoom(st, 0.8),
    "dao.changeState.On" => |st| change_state(st, LayerState::On),
    "dao.changeState.Off" => |st| change_state(st, LayerState::Off),
    "dao.changeState.Toggle" => |st| change_state(st, LayerState::Toggle),
    "dao.usedData.AllOC" => all_layers,
    "dao.usedData.Layers" => some_layers,
    "dao.simplifyActions" => simplify,
}
