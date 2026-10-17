//! Transient notifications.

use dioxus::prelude::*;

use crate::state::{AppState, ToastKind};

#[component]
pub fn Toasts() -> Element {
    let st = use_context::<AppState>();
    let toasts = st.toasts.read().clone();
    rsx! {
        div { class: "toasts",
            for toast in toasts {
                {
                    let id = toast.id;
                    let class = match toast.kind {
                        ToastKind::Info => "toast",
                        ToastKind::Ok => "toast ok",
                        ToastKind::Warn => "toast warn",
                        ToastKind::Error => "toast error",
                    };
                    rsx! {
                        div { class: "{class}", key: "{id}",
                            div {
                                div { class: "t", "{toast.title}" }
                                if !toast.message.is_empty() { div { class: "m", "{toast.message}" } }
                            }
                            button { class: "x", onclick: move |_| st.toasts_mut().retain(|t| t.id != id), "×" }
                        }
                    }
                }
            }
        }
    }
}
