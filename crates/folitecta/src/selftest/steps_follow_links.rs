//! Interaction step: follow_links — the links of a page are followed with
//! the hand tool: a link to a page goes there; a web link names its address
//! and asks first (cancelled here: nothing opens); a link that would start
//! a program is refused without asking. The push buttons of a form do the
//! same: one with a web address asks, one that submits sends nothing and
//! says where the data would go.

use folitecta_core::Document;

use super::*;

async fn click_link(page: usize, index: usize) {
    let _ = eval(&format!(
        "const p = document.querySelector('[data-page=\"{page}\"]'); const l = p ? p.querySelectorAll('.link-hit')[{index}] : null; if (l) l.click();"
    ))
    .await;
    pause(400).await;
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let made = (|| -> folitecta_core::Result<Document> {
        let mut d = Document::new_blank(3, 300.0, 400.0)?;
        d.add_page_link(0, [20.0, 340.0, 120.0, 360.0], 2)?;
        d.add_uri_link(0, [20.0, 300.0, 120.0, 320.0], "https://example.org/folitecta")?;
        d.add_launch_link(0, [20.0, 260.0, 120.0, 280.0], "setup.exe")?;
        Ok(d)
    })();
    let doc = match made {
        Ok(d) => d,
        Err(e) => {
            cx.record("follow links: the document", false, e.to_string());
            return;
        }
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id);
    st.set_tool(Tool::Hand);
    st.go_to_page(0);
    pause(800).await;
    let hits = eval("const p = document.querySelector('[data-page=\"0\"]'); return p ? p.querySelectorAll('.link-hit').length : 0;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    click_link(0, 0).await;
    let page = st.current_page();
    st.go_to_page(0);
    pause(500).await;

    click_link(0, 1).await;
    let asked = match st.dialog.peek().clone() {
        Dialog::OpenTarget { target, .. } => Some(target),
        _ => None,
    };
    let named = eval("return document.querySelector('.dialog .open-target')?.textContent || '';")
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    st.set_dialog(Dialog::None);
    pause(200).await;

    let toasts_before = st.toasts.peek().len();
    click_link(0, 2).await;
    let program_dialog = !matches!(*st.dialog.peek(), Dialog::None);
    let refused = st.toasts.peek().len() > toasts_before;
    cx.record(
        "follow links: a page link goes there, a web link asks first naming it, a program is refused",
        hits == 3
            && page == 2
            && asked == Some(folitecta_core::LinkTarget::Uri("https://example.org/folitecta".into()))
            && named == "https://example.org/folitecta"
            && !program_dialog
            && refused,
        format!(
            "links {hits}, page after the link {page}, asked {asked:?}, named {named:?}, dialog for the program {program_dialog}, refused {refused}"
        ),
    );
    if let Some(id) = tab {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        pause(300).await;
    }

    let Ok(form) = Document::from_bytes(folitecta_core::testdocs::action_buttons_doc(), None, None) else {
        cx.record("follow links: the form with buttons", false, String::new());
        return;
    };
    crate::files::open_document_tab(st, form);
    let tab = st.read_active(|t| t.id);
    st.set_tool(Tool::Hand);
    pause(800).await;
    let _ = eval("document.querySelector('.field-hit[title=\"Go\"]')?.click();").await;
    pause(400).await;
    let go = match st.dialog.peek().clone() {
        Dialog::OpenTarget { target, .. } => Some(target),
        _ => None,
    };
    st.set_dialog(Dialog::None);
    pause(200).await;
    let _ = eval("document.querySelector('.field-hit[title=\"Send\"]')?.click();").await;
    pause(400).await;
    let send_dialog = !matches!(*st.dialog.peek(), Dialog::None);
    let said = st
        .toasts
        .peek()
        .iter()
        .any(|t| t.title.contains("https://example.org/form"));
    cx.record(
        "follow links: a button with a web address asks first, a button that submits sends nothing and says where",
        go == Some(folitecta_core::LinkTarget::Uri("https://example.org/go".into())) && !send_dialog && said,
        format!("asked {go:?}, dialog for the submit {send_dialog}, address named {said}"),
    );
    if let Some(id) = tab {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        pause(300).await;
    }
}
