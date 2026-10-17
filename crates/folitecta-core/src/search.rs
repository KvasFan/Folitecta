//! Search beyond a phrase in the page text: words near each other, and
//! comments, bookmarks, form fields, document information, attachments
//! (including the text of PDF attachments) and links.

use std::collections::BTreeSet;

use crate::document::Document;
use crate::outline::OutlineItem;
use crate::text::{SearchHit, SearchOptions, make_hit};
use crate::textlayer::find_in;

/// How the words of a query must be found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Proximity {
    /// The query as a phrase.
    #[default]
    Phrase,
    /// Every word, close to the others (within a few words).
    Adjacent,
    /// Every word in the same paragraph.
    Paragraph,
    /// Every word on the same page.
    Page,
    /// Every word somewhere in the document.
    Document,
}

/// Where to look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchScopes {
    pub page_text: bool,
    pub comments: bool,
    pub bookmarks: bool,
    pub fields: bool,
    pub doc_info: bool,
    pub attachments: bool,
    pub links: bool,
}

impl Default for SearchScopes {
    fn default() -> Self {
        SearchScopes {
            page_text: true,
            comments: false,
            bookmarks: false,
            fields: false,
            doc_info: false,
            attachments: false,
            links: false,
        }
    }
}

/// Where a match outside the page text was found.
#[derive(Debug, Clone, PartialEq)]
pub enum FoundIn {
    Comment {
        page: usize,
        index: usize,
        subtype: String,
    },
    Bookmark {
        path: Vec<usize>,
        page: Option<usize>,
    },
    Field {
        name: String,
        page: Option<usize>,
    },
    DocInfo {
        key: String,
    },
    Attachment {
        name: String,
    },
    /// The text of a PDF attachment.
    AttachmentPage {
        name: String,
        page: usize,
    },
    Link {
        page: usize,
        index: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct OtherHit {
    pub found_in: FoundIn,
    /// The matching text, shortened around the match.
    pub snippet: String,
}

fn snippet_of(text: &str, start: usize, end: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let from = start.saturating_sub(40);
    let to = (end + 40).min(chars.len());
    let mut s: String = chars[from.min(to)..to]
        .iter()
        .map(|c| if c.is_whitespace() { ' ' } else { *c })
        .collect();
    if from > 0 {
        s.insert(0, '…');
    }
    if to < chars.len() {
        s.push('…');
    }
    s
}

/// Where `query` matches `text`: the phrase, or (other modes) every word
/// anywhere in the text; the range of the first match.
pub fn match_text(
    text: &str,
    query: &str,
    opts: SearchOptions,
    proximity: Proximity,
) -> Option<(usize, usize)> {
    let words: Vec<&str> = query.split_whitespace().collect();
    if proximity == Proximity::Phrase || words.len() < 2 {
        return find_in(text, query, opts).first().copied();
    }
    let mut first = None;
    for w in words {
        let hit = *find_in(text, w, opts).first()?;
        first = Some(first.map_or(hit, |f: (usize, usize)| if hit.0 < f.0 { hit } else { f }));
    }
    first
}

fn walk_outline(
    items: &[OutlineItem],
    path: &mut Vec<usize>,
    out: &mut Vec<(Vec<usize>, String, Option<usize>)>,
) {
    for (i, it) in items.iter().enumerate() {
        path.push(i);
        out.push((path.clone(), it.title.clone(), it.page));
        walk_outline(&it.children, path, out);
        path.pop();
    }
}

impl Document {
    /// Find the words of `query` on the pages, as a phrase or near each other.
    /// Hits of word searches cover each word and tell which word it is.
    pub fn search_words(&self, query: &str, opts: SearchOptions, proximity: Proximity) -> Vec<SearchHit> {
        let words: Vec<&str> = query.split_whitespace().collect();
        if proximity == Proximity::Phrase || words.len() < 2 {
            return self.search(query, opts);
        }
        let layers = self.text_layers();
        let per_page: Vec<Vec<Vec<(usize, usize)>>> = layers
            .iter()
            .map(|l| words.iter().map(|w| l.find(w, opts)).collect())
            .collect();
        if proximity == Proximity::Document
            && !(0..words.len()).all(|w| per_page.iter().any(|p| !p[w].is_empty()))
        {
            return Vec::new();
        }
        let mut hits = Vec::new();
        for (page, layer) in layers.iter().enumerate() {
            let occ = &per_page[page];
            let mut keep: BTreeSet<(usize, usize, usize)> = BTreeSet::new();
            let all_of = |sets: &[Vec<(usize, usize)>], keep: &mut BTreeSet<(usize, usize, usize)>| {
                if sets.iter().all(|l| !l.is_empty()) {
                    for (w, l) in sets.iter().enumerate() {
                        keep.extend(l.iter().map(|(s, e)| (*s, *e, w)));
                    }
                }
            };
            match proximity {
                Proximity::Document => {
                    for (w, l) in occ.iter().enumerate() {
                        keep.extend(l.iter().map(|(s, e)| (*s, *e, w)));
                    }
                }
                Proximity::Page => all_of(occ, &mut keep),
                Proximity::Paragraph => {
                    for (ps, pe) in layer.paragraphs() {
                        let inside: Vec<Vec<(usize, usize)>> = occ
                            .iter()
                            .map(|l| l.iter().filter(|(s, _)| *s >= ps && *s < pe).copied().collect())
                            .collect();
                        all_of(&inside, &mut keep);
                    }
                }
                Proximity::Adjacent => {
                    // Positions in words, so the distance counts words.
                    let starts: Vec<usize> = layer.words().iter().map(|(s, _)| *s).collect();
                    let pos = |c: usize| starts.partition_point(|s| *s <= c).saturating_sub(1);
                    let mut all: Vec<(usize, usize, usize, usize)> = occ
                        .iter()
                        .enumerate()
                        .flat_map(|(w, l)| {
                            l.iter()
                                .map(move |(s, e)| (pos(*s), *s, *e, w))
                                .collect::<Vec<_>>()
                        })
                        .collect();
                    all.sort_unstable();
                    let span = words.len() + 3;
                    for i in 0..all.len() {
                        let mut seen = vec![false; words.len()];
                        let mut j = i;
                        while j < all.len() && all[j].0 <= all[i].0 + span {
                            seen[all[j].3] = true;
                            if seen.iter().all(|x| *x) {
                                keep.extend(all[i..=j].iter().map(|o| (o.1, o.2, o.3)));
                                break;
                            }
                            j += 1;
                        }
                    }
                }
                Proximity::Phrase => unreachable!("handled above"),
            }
            let chars = layer.chars();
            for (index, (s, e, w)) in keep.into_iter().enumerate() {
                hits.push(make_hit(page, index, layer, &chars, s, e, w));
            }
        }
        hits
    }

    /// Find `query` outside the page text, in the places `scopes` turns on.
    pub fn search_other(
        &self,
        query: &str,
        opts: SearchOptions,
        proximity: Proximity,
        scopes: &SearchScopes,
    ) -> Vec<OtherHit> {
        let mut out = Vec::new();
        if query.trim().is_empty() {
            return out;
        }
        let test = |text: &str| match_text(text, query, opts, proximity).map(|(s, e)| snippet_of(text, s, e));
        if scopes.comments {
            for page in 0..self.page_count() {
                for a in self.annotations(page).unwrap_or_default() {
                    if matches!(a.subtype.as_str(), "Link" | "Widget" | "Popup") {
                        continue;
                    }
                    let found = test(&a.contents)
                        .or_else(|| test(&a.subject))
                        .or_else(|| test(&a.author));
                    if let Some(snippet) = found {
                        out.push(OtherHit {
                            found_in: FoundIn::Comment {
                                page,
                                index: a.index,
                                subtype: a.subtype.clone(),
                            },
                            snippet,
                        });
                    }
                }
            }
        }
        if scopes.bookmarks {
            let mut items = Vec::new();
            walk_outline(&self.outline(), &mut Vec::new(), &mut items);
            for (path, title, page) in items {
                if let Some(snippet) = test(&title) {
                    out.push(OtherHit {
                        found_in: FoundIn::Bookmark { path, page },
                        snippet,
                    });
                }
            }
        }
        if scopes.fields {
            for f in self.form_fields() {
                if let Some(snippet) = test(&f.value).or_else(|| test(&f.name)) {
                    out.push(OtherHit {
                        found_in: FoundIn::Field {
                            name: f.name.clone(),
                            page: f.page,
                        },
                        snippet,
                    });
                }
            }
        }
        if scopes.doc_info {
            let info = self.info();
            for (key, value) in [
                ("Title", &info.title),
                ("Author", &info.author),
                ("Subject", &info.subject),
                ("Keywords", &info.keywords),
                ("Creator", &info.creator),
                ("Producer", &info.producer),
            ] {
                if let Some(snippet) = test(value) {
                    out.push(OtherHit {
                        found_in: FoundIn::DocInfo { key: key.into() },
                        snippet,
                    });
                }
            }
        }
        if scopes.attachments {
            for a in self.attachments() {
                if let Some(snippet) = test(&a.name).or_else(|| test(&a.description)) {
                    out.push(OtherHit {
                        found_in: FoundIn::Attachment { name: a.name.clone() },
                        snippet,
                    });
                }
                let is_pdf = a.name.to_lowercase().ends_with(".pdf") || a.mime == "application/pdf";
                if !is_pdf {
                    continue;
                }
                let Ok(data) = self.attachment_data(&a) else {
                    continue;
                };
                let Ok(inner) = Document::from_bytes(data, None, None) else {
                    continue;
                };
                for h in inner.search_words(query, opts, proximity) {
                    out.push(OtherHit {
                        found_in: FoundIn::AttachmentPage {
                            name: a.name.clone(),
                            page: h.page,
                        },
                        snippet: h.snippet,
                    });
                }
            }
        }
        if scopes.links {
            for l in self.all_links() {
                if let Some(snippet) = test(&l.target.describe()) {
                    out.push(OtherHit {
                        found_in: FoundIn::Link {
                            page: l.page,
                            index: l.index,
                        },
                        snippet,
                    });
                }
            }
        }
        out
    }
}
