//! Interaction step 47: barcodes — the barcode tool asks for the kind and
//! content of a barcode, the barcodes of the document are read back and one
//! is shown on its page, and a web link is made over the one that holds an
//! address.

use super::*;

const ADDRESS: &str = "https://github.com/KvasFan/Folitecta";

/// Wait for the work started away from the interface thread (at most 20 s).
async fn settle(st: AppState) {
    pause(200).await;
    for _ in 0..100 {
        if !*st.busy.peek() {
            break;
        }
        pause(200).await;
    }
    pause(300).await;
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    // 47. The tool, then its dialog filled in and confirmed.
    crate::commands::dispatch(st, "tool.selBarcodePlace");
    let tool_on = (st.tool)() == Tool::Barcode;
    // An empty part of the second page.
    st.set_dialog(Dialog::Barcode {
        page: 1,
        rect: [300.0, 300.0, 420.0, 420.0],
    });
    pause(400).await;
    let script = format!(
        r#"const d = document.querySelector('.dialog');
const sel = d?.querySelector('select');
if (sel) {{ sel.value = 'qr'; sel.dispatchEvent(new Event('change', {{ bubbles: true }})); }}
const inp = d?.querySelector('input[type=text]');
if (inp) {{ inp.value = {address}; inp.dispatchEvent(new Event('input', {{ bubbles: true }})); }}"#,
        address = serde_json::to_string(ADDRESS).unwrap_or_default()
    );
    let _ = document::eval(&script).await;
    pause(300).await;
    let _ = capture_window(&cx.dir.join("interaction-barcode.png"));
    let _ = document::eval("document.querySelector('.dialog .tbtn.primary')?.click();").await;
    pause(700).await;
    let placed = matches!((st.dialog)(), Dialog::None) && (st.tool)() == Tool::Hand;

    // Every page read: the dialog lists the code; Show goes to it.
    crate::commands::dispatch(st, "document.detectImageBarcode");
    settle(st).await;
    let listed = document::eval(&format!(
        "return [...document.querySelectorAll('.dialog .barcode-row')].filter(r => r.textContent.includes({})).length;",
        serde_json::to_string(ADDRESS).unwrap_or_default()
    ))
    .await
    .ok()
    .and_then(|v| v.as_u64())
    .unwrap_or(0);
    let _ = capture_window(&cx.dir.join("interaction-barcodes-found.png"));
    let _ = document::eval("document.querySelector('.dialog .barcode-show')?.click();").await;
    pause(400).await;
    let shown = st
        .read_active(|t| {
            t.flash
                .is_some_and(|(p, r)| p == 1 && r[0] > 295.0 && r[2] < 425.0)
        })
        .unwrap_or(false);
    st.set_dialog(Dialog::None);

    // A web link over it, in one step.
    let links = || {
        st.read_active(|t| t.doc.links(1).map(|l| l.len()).unwrap_or(0))
            .unwrap_or(0)
    };
    let before = links();
    crate::commands::dispatch(st, "createWebLinksFromBarcodes");
    settle(st).await;
    let after = links();
    cx.record(
        "barcodes: one drawn with the tool, read back, shown, and a web link over it",
        tool_on && placed && listed == 1 && shown && after == before + 1,
        format!("tool {tool_on}, placed {placed}, listed {listed}, shown {shown}, links {before} -> {after}"),
    );
}
