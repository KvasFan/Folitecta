//! The look of the interface: the style sheets of each part, put together in
//! the order of the cascade. Colours come from the theme tokens of
//! `assets/styles/theme.css`; [`crate::state::Theme`] picks the theme.

/// Every style sheet, in order: theme tokens, base and shared controls, the
/// parts of the window, the features, then printing and the responsive rules
/// that override the rest.
pub const CSS: &str = concat!(
    include_str!("../assets/styles/theme.css"),
    include_str!("../assets/styles/base.css"),
    include_str!("../assets/styles/controls.css"),
    include_str!("../assets/styles/menubar.css"),
    include_str!("../assets/styles/ribbon.css"),
    include_str!("../assets/styles/panes.css"),
    include_str!("../assets/styles/canvas.css"),
    include_str!("../assets/styles/statusbar.css"),
    include_str!("../assets/styles/welcome.css"),
    include_str!("../assets/styles/dialogs.css"),
    include_str!("../assets/styles/features/signatures.css"),
    include_str!("../assets/styles/features/macros.css"),
    include_str!("../assets/styles/features/portfolio.css"),
    include_str!("../assets/styles/features/stamps.css"),
    include_str!("../assets/styles/features/doctools.css"),
    include_str!("../assets/styles/features/compare.css"),
    include_str!("../assets/styles/features/tables.css"),
    include_str!("../assets/styles/features/forms.css"),
    include_str!("../assets/styles/features/accessibility.css"),
    include_str!("../assets/styles/features/calculator.css"),
    include_str!("../assets/styles/features/metadata.css"),
    include_str!("../assets/styles/features/javascript.css"),
    include_str!("../assets/styles/features/media.css"),
    include_str!("../assets/styles/features/sources.css"),
    include_str!("../assets/styles/features/pictures.css"),
    include_str!("../assets/styles/features/split.css"),
    include_str!("../assets/styles/features/comment-styles.css"),
    include_str!("../assets/styles/features/recovery.css"),
    include_str!("../assets/styles/features/barcodes.css"),
    include_str!("../assets/styles/features/spelling.css"),
    include_str!("../assets/styles/features/ocr.css"),
    include_str!("../assets/styles/features/scans.css"),
    include_str!("../assets/styles/features/policies.css"),
    include_str!("../assets/styles/features/mail.css"),
    include_str!("../assets/styles/features/print.css"),
    include_str!("../assets/styles/print.css"),
    include_str!("../assets/styles/responsive.css"),
);

#[cfg(test)]
mod tests {
    use super::*;

    /// The sheets close every rule they open, and each theme other than the
    /// default has its tokens.
    #[test]
    fn braces_balance_and_every_theme_defines_its_tokens() {
        assert_eq!(CSS.matches('{').count(), CSS.matches('}').count());
        let theme = include_str!("../assets/styles/theme.css");
        for t in crate::state::Theme::ALL {
            let sel = format!("[data-theme=\"{}\"]", t.attr());
            assert!(
                t == crate::state::Theme::Light || theme.contains(&sel),
                "no tokens for {sel}"
            );
        }
    }
}
