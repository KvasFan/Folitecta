//! Barcodes: the tools that draw one or read those of an area, reading the
//! barcodes of the document, and web links over those that hold an address.
//! Pages are read away from the interface thread.

use dioxus::prelude::*;
use folitecta_core::FoundBarcode;
use folitecta_core::barcodes::find_barcodes;

use super::{handlers, no_doc, select_tool};
use crate::files;
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind, Tool};

/// Read the barcodes of `pages` of the active document (every page when
/// `None`), or of an area of one page; `then` gets what was found.
fn read(
    st: AppState,
    pages: Option<Vec<usize>>,
    region: Option<[f32; 4]>,
    then: fn(AppState, Vec<FoundBarcode>),
) {
    let Some((bytes, n)) = st.read_active(|t| (t.doc.bytes(), t.doc.page_count())) else {
        return;
    };
    let pages = pages.unwrap_or_else(|| (0..n).collect());
    st.set_busy(true);
    spawn(async move {
        let found = tokio::task::spawn_blocking(move || find_barcodes(bytes, &pages, region)).await;
        st.set_busy(false);
        match found {
            Ok(Ok(found)) => then(st, found),
            Ok(Err(e)) => st.error(e),
            Err(e) => st.error(e),
        }
    });
}

fn show(st: AppState, found: Vec<FoundBarcode>) {
    if found.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_barcodes"), "");
    } else {
        st.set_dialog(Dialog::Barcodes(found));
    }
}

fn links(st: AppState, found: Vec<FoundBarcode>) {
    match st.with_active(|t| t.doc.create_links_over(&found)) {
        Some(Ok(n)) => {
            files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.barcode_links", &[("count", &n.to_string())]),
                "",
            );
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

/// Read the barcodes of an area of a page (display points, y down).
pub fn read_area(st: AppState, page: usize, rect: [f32; 4]) {
    read(st, Some(vec![page]), Some(rect), show);
}

/// Go to a barcode that was found and show where it is.
pub fn reveal(st: AppState, found: &FoundBarcode) {
    st.go_to_page(found.page);
    st.with_active(|t| t.flash = Some((found.page, found.rect)));
}

handlers! {
    "tool.selBarcodePlace" => |st| select_tool(st, Tool::Barcode),
    "document.detectBarcodes" => |st| select_tool(st, Tool::BarcodeScan),
    "document.detectImageBarcode" => |st| { if !no_doc(st) { read(st, None, None, show) } },
    "createWebLinksFromBarcodes" => |st| { if !no_doc(st) { read(st, None, None, links) } },
}
