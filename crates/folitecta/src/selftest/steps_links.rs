//! Interaction step 50: the links pane — a click selects a link and shows
//! it on its page, its address is copied from the pane menu, long addresses
//! can wrap — and View › Expand All / Collapse All on the bookmarks pane.

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    // 50. The links pane, its first web link selected.
    {
        let mut ui = st.ui_mut();
        ui.show_left_pane = true;
        ui.left_pane = "links".into();
    }
    pause(500).await;
    let clicked = document::eval(
        "const it = [...document.querySelectorAll('.pane-panel .list-item')].find(i => i.textContent.includes('http')); if (!it) return false; it.click(); return true;",
    )
    .await
    .ok()
    .and_then(|v| v.as_bool())
    .unwrap_or(false);
    pause(500).await;
    let (selected, flashed) = st
        .read_active(|t| (t.selected_link, t.flash.is_some()))
        .unwrap_or((None, false));
    let address = selected.and_then(|(page, index)| {
        st.read_active(|t| {
            t.doc
                .links(page)
                .ok()?
                .into_iter()
                .find(|l| l.index == index)
                .and_then(|l| match l.target {
                    folitecta_core::LinkTarget::Uri(u) => Some(u),
                    _ => None,
                })
        })
        .flatten()
    });
    crate::commands::dispatch(st, "linksView.link.copyURL");
    pause(300).await;
    let copied = crate::clipboard::get_text();
    crate::commands::dispatch(st, "linksView.wrapTitles");
    pause(300).await;
    let wrapped =
        document::eval("return document.querySelectorAll('.pane-panel .list-item .body.wrap').length;")
            .await
            .ok()
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
    let _ = capture_window(&cx.dir.join("interaction-links-pane.png"));
    crate::commands::dispatch(st, "linksView.wrapTitles");

    // Expand All and Collapse All act on the bookmarks pane.
    st.ui_mut().left_pane = "bookmarks".into();
    pause(300).await;
    crate::commands::dispatch(st, "expandAll");
    let expanded = st
        .read_active(|t| t.expanded_bookmarks.contains("*"))
        .unwrap_or(false);
    crate::commands::dispatch(st, "collapseAll");
    let collapsed = st
        .read_active(|t| t.expanded_bookmarks.is_empty())
        .unwrap_or(false);
    st.ui_mut().left_pane = "thumbnails".into();
    st.with_active(|t| {
        t.selected_link = None;
        t.flash = None;
    });
    cx.record(
        "links pane: a link selected and shown, its address copied, long addresses wrapped; expand and collapse all",
        clicked && flashed && address.is_some() && copied == address && wrapped > 0 && expanded && collapsed,
        format!(
            "clicked {clicked}, selected {selected:?}, flashed {flashed}, address {address:?}, copied {copied:?}, wrapped {wrapped}, expanded {expanded}, collapsed {collapsed}"
        ),
    );
}
