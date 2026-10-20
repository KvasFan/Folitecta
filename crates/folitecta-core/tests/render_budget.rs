//! The pictures the render threads keep stay within one budget for every
//! open document together (128 MB), however many documents are open and in
//! whatever order they were drawn.

use std::sync::Arc;

use folitecta_core::{Lane, RenderAnswer, RenderOpts, RenderRequest, RenderService};

const BUDGET: usize = 128 * 1024 * 1024;

/// Ask for a page; the bytes of its picture and whether it was drawn (not kept).
fn ask(svc: &RenderService, page: usize, scale: f32) -> (usize, bool) {
    let before = svc.stats().rendered;
    let (tx, rx) = std::sync::mpsc::channel();
    svc.request(
        RenderRequest {
            page,
            scale,
            opts: RenderOpts::default(),
            lane: Lane::Other,
            rev: None,
            tile: None,
        },
        move |a| {
            let _ = tx.send(a);
        },
    );
    let RenderAnswer::Picture(p) = rx.recv().unwrap() else {
        panic!("no picture of page {page}");
    };
    (p.png.len(), svc.stats().rendered != before)
}

/// The bytes of the pictures a document still keeps, counted from the page
/// asked last (asking again a kept picture keeps it; the first one not kept
/// ends the count).
fn kept_bytes(svc: &RenderService, pages: usize, scale: f32) -> usize {
    let mut total = 0;
    for page in (0..pages).rev() {
        let (bytes, drawn) = ask(svc, page, scale);
        if drawn {
            break;
        }
        total += bytes;
    }
    total
}

#[test]
fn documents_drawn_one_after_the_other_stay_within_one_budget() {
    // Pages of about 6 MB (text at 200 %, not compressed): 50 of them pass
    // the budget more than twice.
    let (pages, scale) = (50, 2.0f32);
    let bytes = Arc::new(folitecta_core::testdocs::text_doc(pages));
    let first = RenderService::with_workers(bytes.clone(), 2);
    for page in 0..pages {
        ask(&first, page, scale);
    }
    let second = RenderService::with_workers(bytes, 2);
    for page in 0..pages {
        ask(&second, page, scale);
    }
    let kept = kept_bytes(&second, pages, scale) + kept_bytes(&first, pages, scale);
    assert!(
        kept <= BUDGET,
        "the two documents keep {} MB of pictures",
        kept / (1024 * 1024)
    );
    // The budget is used: the pictures drawn last are kept.
    assert!(kept > BUDGET / 2, "only {} MB kept", kept / (1024 * 1024));
}
