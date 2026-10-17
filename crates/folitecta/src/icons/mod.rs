//! Icons of the interface: [`set`] names the drawings (Tabler Icons, MIT),
//! [`commands`] chooses the icon of each command, and [`Icon`] draws one.

mod commands;
mod set;

use dioxus::prelude::*;

pub use commands::for_command;

/// An icon by name, drawn with the current text colour.
#[component]
pub fn Icon(name: &'static str, #[props(default = String::new())] class: String) -> Element {
    let d = set::data(name);
    rsx! {
        svg { class: "ic {class}", view_box: "0 0 24 24", "aria-hidden": "true", dangerous_inner_html: "{d.data}" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_has_a_known_icon() {
        for c in folitecta_core::registry().all() {
            let name = for_command(&c.id);
            assert!(set::NAMES.contains(&name), "{} has the unknown icon {name}", c.id);
        }
    }

    #[test]
    fn every_name_has_its_own_drawing() {
        let fallback = set::data("no such icon");
        for name in set::NAMES {
            assert!(!set::data(name).data.is_empty(), "{name}");
            // Only the content icon may share the drawing of unknown names.
            if *name != "content" {
                assert!(!std::ptr::eq(set::data(name), fallback), "{name} has no drawing");
            }
        }
        assert_eq!(for_command("tool.hand"), "hand");
        assert_eq!(for_command("tool.annot.underline"), "underline");
        assert_eq!(for_command("pageText.delete"), "text-delete");
    }
}
