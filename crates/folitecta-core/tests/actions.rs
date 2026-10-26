//! Action sequences: steps run on a document as one undo step; a step that
//! fails leaves the document as it was, with the log so far; a batch over
//! files and folders planned with its collisions, every file with a result
//! (done, failed — a protected file —, skipped — an output that exists),
//! the outputs written and the inputs untouched.

use std::path::PathBuf;

use folitecta_core::actions::{batch_inputs, plan_batch, run_batch_item};
use folitecta_core::{
    ActionSequence, Document, EncryptionProfile, OutputRule, PageSet, SecurityPermissions, Step, TextLayout,
    security::protected_copy,
};

fn report(pages: usize) -> Document {
    let p = "Plain text of a page. ".repeat(60);
    let md = (0..pages)
        .map(|i| format!("# Part {}\n\n{p}\n\n", i + 1))
        .collect::<String>();
    Document::from_markdown(&md, TextLayout::default(), "Report").unwrap()
}

fn sequence() -> ActionSequence {
    ActionSequence {
        name: "Review copy".into(),
        steps: vec![
            Step::Rotate {
                degrees: 90,
                pages: PageSet::Odd,
            },
            Step::Watermark {
                text: "REVIEW".into(),
                opacity: 0.3,
            },
            Step::PageNumbers {
                format: "Page {page} of {pages}".into(),
                position: "bottom-right".into(),
            },
            Step::Info {
                title: "Checked report".into(),
                author: String::new(),
                subject: String::new(),
                keywords: String::new(),
            },
        ],
    }
}

#[test]
fn a_sequence_runs_as_one_undo_step() {
    let mut doc = report(3);
    doc.clear_history();
    let before = doc.bytes();
    let log = doc.run_actions(&sequence()).unwrap();
    assert_eq!(log.len(), 4, "{log:?}");
    assert_eq!(doc.page_size(0).unwrap().rotation, 90);
    assert_eq!(doc.page_size(1).unwrap().rotation, 0);
    let text = doc.page_text(1);
    assert!(text.contains("REVIEW") && text.contains("Page 2 of"), "{text}");
    assert_eq!(doc.info().title, "Checked report");
    doc.undo().unwrap();
    assert_eq!(doc.bytes(), before, "one undo takes the whole sequence back");
    assert!(!doc.can_undo());
}

#[test]
fn a_failing_step_leaves_the_document_as_it_was() {
    let mut doc = report(2);
    doc.clear_history();
    let before = doc.bytes();
    let mut seq = sequence();
    seq.steps.insert(
        2,
        Step::Recompress {
            preset: "nonsense".into(),
        },
    );
    let (log, err) = doc.run_actions(&seq).unwrap_err();
    assert_eq!(log.len(), 3, "{log:?}");
    assert!(log[2].contains("recompress") && err.to_string().contains("nonsense"));
    assert_eq!(doc.bytes(), before);
}

#[test]
fn a_batch_has_a_result_for_every_file() {
    let dir = std::env::temp_dir().join(format!("folitecta-actions-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let sub = dir.join("in").join("more");
    std::fs::create_dir_all(&sub).unwrap();
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let a = dir.join("in").join("a.pdf");
    let b = sub.join("b.pdf");
    let locked = dir.join("in").join("locked.pdf");
    report(2).save_as(&a).unwrap();
    report(1).save_as(&b).unwrap();
    let plain = report(1);
    let bytes = protected_copy(
        plain.bytes().as_slice(),
        "secret",
        "owner",
        SecurityPermissions::default(),
        EncryptionProfile::Aes256,
    )
    .unwrap();
    std::fs::write(&locked, bytes).unwrap();
    std::fs::write(dir.join("in").join("notes.txt"), "not a PDF").unwrap();
    // An output already there.
    std::fs::write(out.join("b-review.pdf"), "old").unwrap();
    let a_before = std::fs::read(&a).unwrap();

    let inputs = batch_inputs(&[dir.join("in")]);
    assert_eq!(
        inputs,
        vec![a.clone(), locked.clone(), b.clone()],
        "PDF files only, subfolders included"
    );
    let rule = OutputRule {
        folder: Some(out.clone()),
        suffix: "-review".into(),
        overwrite: false,
    };
    let plan = plan_batch(&inputs, &rule);
    let collisions: Vec<Option<&str>> = plan.iter().map(|i| i.collision).collect();
    assert_eq!(collisions, vec![None, None, Some("exists")]);
    let results: Vec<_> = plan
        .iter()
        .map(|item| run_batch_item(&sequence(), item, &rule))
        .collect();
    let status: Vec<&str> = results.iter().map(|r| r.status).collect();
    assert_eq!(status, vec!["done", "failed", "skipped"], "{results:?}");
    assert!(results[1].message.contains("password"));
    assert_eq!(results[0].log.len(), 4);
    let made = Document::open(out.join("a-review.pdf")).unwrap();
    assert!(made.page_text(0).contains("REVIEW"));
    assert_eq!(std::fs::read(&a).unwrap(), a_before, "the input is untouched");
    assert_eq!(std::fs::read(out.join("b-review.pdf")).unwrap(), b"old");
    // With overwrite the existing output is replaced; the same file as output is planned as such.
    let rule2 = OutputRule {
        overwrite: true,
        ..rule.clone()
    };
    let again = run_batch_item(
        &sequence(),
        &plan_batch(std::slice::from_ref(&b), &rule2)[0],
        &rule2,
    );
    assert_eq!(again.status, "done");
    let same = plan_batch(
        std::slice::from_ref(&a),
        &OutputRule {
            folder: None,
            suffix: String::new(),
            overwrite: false,
        },
    );
    assert_eq!(same[0].collision, Some("same-file"));
    assert_eq!(same[0].output, a);
    let twice = plan_batch(&[a.clone(), PathBuf::from(&a)], &rule);
    assert_eq!(twice[1].collision, Some("twice"));
    std::fs::remove_dir_all(&dir).ok();
}
