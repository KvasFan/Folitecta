//! Spell checking with the dictionaries of the source tree: words checked in
//! two languages at once, suggestions, the user's words, and the words of the
//! page text, comments and fields corrected in place.

use std::path::PathBuf;
use std::sync::OnceLock;

use folitecta_core::spelling::find_dictionaries;
use folitecta_core::{
    Document, FieldProps, NewField, SpellOptions, SpellPlace, SpellScope, Speller, TextLayout,
};

fn dictionaries() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dictionaries")
}

/// English and Italian, loaded once for every test.
fn speller() -> &'static Speller {
    static SPELLER: OnceLock<Speller> = OnceLock::new();
    SPELLER.get_or_init(|| {
        let mut s = Speller::new();
        for f in find_dictionaries(&[dictionaries()]) {
            let t = std::time::Instant::now();
            s.load(&f).unwrap();
            eprintln!("{} loaded in {:?}", f.code, t.elapsed());
        }
        s
    })
}

/// A checker of its own, for the tests that change the words or options.
fn english() -> Speller {
    let files = find_dictionaries(&[dictionaries()]);
    let mut s = Speller::new();
    s.load(files.iter().find(|f| f.code == "en_US").unwrap()).unwrap();
    s
}

fn wrong(s: &Speller, text: &str) -> Vec<String> {
    s.misspelled_in(text).into_iter().map(|(_, _, w)| w).collect()
}

#[test]
fn the_dictionaries_of_the_source_tree_are_found() {
    let codes: Vec<String> = find_dictionaries(&[dictionaries()])
        .into_iter()
        .map(|f| f.code)
        .collect();
    assert_eq!(codes, ["en_US", "it_IT"]);
    assert_eq!(speller().languages(), ["en_US", "it_IT"]);
}

#[test]
fn words_are_checked_in_both_languages() {
    let s = speller();
    for w in [
        "hello",
        "World",
        "optimize",
        "don't",
        "perché",
        "città",
        "l'albero",
        "dell'anno",
        "L’albero",
    ] {
        assert!(s.check(w), "{w} should be right");
    }
    for w in ["helo", "wrold", "albro", "perce"] {
        assert!(!s.check(w), "{w} should be wrong");
    }
    assert!(wrong(s, "Il gatto è sul tavolo and the cat is on the table.").is_empty());
    assert_eq!(
        wrong(s, "This is a smple sentence, un esempio sbagliatto."),
        ["smple", "sbagliatto"]
    );
}

#[test]
fn what_the_check_passes_over() {
    let s = english();
    // Capitals, digits, addresses, names written with inner capitals, a word
    // divided at the end of a line, single letters.
    let text = "NASA and PDF files, item 42b, see www.exmaple.com or mail ab@cd.ef, \
                the iPhone, an exam-\nple, x y z.";
    assert!(wrong(&s, text).is_empty(), "{:?}", wrong(&s, text));
    s.set_options(SpellOptions {
        skip_capitals: false,
        skip_digits: false,
    });
    assert_eq!(wrong(&s, "NASA QXZW abc123"), ["QXZW", "abc123"]);
}

#[test]
fn suggestions_come_from_every_dictionary() {
    let s = speller();
    let t = std::time::Instant::now();
    let en = s.suggest("smple");
    let it = s.suggest("perche");
    eprintln!("suggestions in {:?}", t.elapsed());
    assert!(en.iter().any(|w| w == "simple" || w == "sample"), "{en:?}");
    assert!(it.iter().any(|w| w == "perché"), "{it:?}");
    // Swapped letters and accents first, then the language of the text around.
    let the = s.suggest_in("Teh", "Teh quick brown fox jumps over the lazy dog.");
    assert_eq!(the.first().map(String::as_str), Some("The"), "{the:?}");
    let wrong_it = s.suggest_in("sbagliatto", "Un esempio sbagliatto di testo italiano.");
    assert_eq!(
        wrong_it.first().map(String::as_str),
        Some("sbagliato"),
        "{wrong_it:?}"
    );
    assert_eq!(s.suggest("perche").first().map(String::as_str), Some("perché"));
}

#[test]
fn the_users_words_and_ignored_words_are_right() {
    let s = english();
    let before = s.changes();
    assert!(!s.check("Folitecta"));
    assert!(s.add_word("Folitecta"));
    assert!(!s.add_word("Folitecta"));
    // A name: right with its capital and in capitals, not in small letters.
    assert!(s.check("Folitecta") && s.check("FOLITECTA") && !s.check("folitecta"));
    s.add_word("foliate");
    assert!(s.check("Foliate") && s.check("FOLIATE"));
    assert_eq!(s.added_words(), ["foliate", "Folitecta"]);
    s.ignore_word("qwzx");
    assert!(s.check("qwzx"));
    assert!(s.remove_word("Folitecta"));
    assert!(!s.check("Folitecta"));
    assert!(s.changes() > before);
}

#[test]
fn without_dictionaries_nothing_is_wrong() {
    let s = Speller::new();
    assert!(!s.has_dictionaries());
    assert!(s.check("qwzx"));
    assert!(s.misspelled_in("qwzx vbnm").is_empty());
}

#[test]
fn words_of_the_pages_comments_and_fields_are_corrected() {
    let s = speller();
    let mut doc = Document::from_text(
        "The quikc brown fox\njumps over the lazzy dog",
        TextLayout::default(),
    )
    .unwrap();
    doc.add_note(
        0,
        300.0,
        700.0,
        "A note with a mistake: teh",
        "Reviewer",
        [1.0, 0.9, 0.2],
    )
    .unwrap();
    let props = FieldProps {
        name: "remark".into(),
        ..FieldProps::default()
    };
    let field = doc
        .add_form_field(0, NewField::Text, [40.0, 100.0, 300.0, 120.0], &props)
        .unwrap();
    doc.set_field_value(field, "Grazie per l'attenzzione").unwrap();

    let found = doc.misspellings(s, SpellScope::default());
    let words: Vec<&str> = found.iter().map(|m| m.word.as_str()).collect();
    // The elided article is part of the word.
    assert_eq!(words, ["quikc", "lazzy", "teh", "l'attenzzione"]);
    assert!(s.suggest("l'attenzzione").iter().any(|w| w == "l'attenzione"));
    assert!(matches!(found[0].place, SpellPlace::Page(0)));
    assert!(matches!(found[2].place, SpellPlace::Comment { page: 0, .. }));
    assert_eq!(found[3].place, SpellPlace::Field(field));
    // Reading order: the page text, its comments, then its fields.
    assert!(found.windows(2).all(|w| w[0].key() < w[1].key()));
    let only_pages = doc.misspellings(
        s,
        SpellScope {
            comments: false,
            fields: false,
            ..SpellScope::default()
        },
    );
    assert_eq!(only_pages.len(), 2);
    assert_eq!(doc.page_misspellings(0, s), only_pages);

    // One word, then the others in one undo step.
    doc.correct_spelling(&found[0], "quick").unwrap();
    assert_eq!(doc.page_text(0), "The quick brown fox jumps over the lazzy dog");
    let rest = doc.misspellings(s, SpellScope::default());
    assert_eq!(rest.len(), 3);
    // A word that is no longer where it was found is not changed.
    assert!(doc.correct_spelling(&found[0], "quick").is_err());
    let r = doc.correct_spellings(&rest[..1], "lazy");
    assert_eq!((r.replaced, r.refused), (1, 0));
    doc.correct_spelling(&rest[1], "the").unwrap();
    doc.correct_spelling(&rest[2], "l'attenzione").unwrap();
    assert!(doc.misspellings(s, SpellScope::default()).is_empty());
    assert_eq!(
        doc.spelling_text(rest[1].place).as_deref(),
        Some("A note with a mistake: the")
    );
    assert_eq!(
        doc.spelling_text(SpellPlace::Field(field)).as_deref(),
        Some("Grazie per l'attenzione")
    );
    doc.undo().unwrap();
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.page_text(0), "The quick brown fox jumps over the lazzy dog");
}

#[test]
fn every_occurrence_is_corrected_in_one_undo_step() {
    let s = speller();
    let mut doc =
        Document::from_text("recieve one, recieve two, recieve three", TextLayout::default()).unwrap();
    let found = doc.misspellings(s, SpellScope::default());
    assert_eq!(found.len(), 3);
    assert!(s.suggest("recieve").iter().any(|w| w == "receive"));
    let r = doc.correct_spellings(&found, "receive");
    assert_eq!(r.replaced, 3);
    assert_eq!(doc.page_text(0), "receive one, receive two, receive three");
    doc.undo().unwrap();
    assert_eq!(doc.page_text(0), "recieve one, recieve two, recieve three");
}
