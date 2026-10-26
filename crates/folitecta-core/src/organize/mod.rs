//! Organising documents: pages and page tools, document tools, combining, comparing, exporting, text, Markdown and CSV documents, e-mail messages and the copies prepared to send, portfolios, attachments, metadata, page marks and the clean-up of scanned pages.

pub mod attachments;
pub mod combine;
pub mod compare;
pub mod csvdoc;
pub mod delivery;
pub mod doctools;
pub mod email;
pub mod export;
pub mod greyscale;
pub mod markdown;
pub mod marks;
pub mod metadata;
pub mod pages;
pub mod pagetools;
pub mod pdfa;
pub mod portfolio;
pub mod print;
pub mod recompress;
pub mod scanning;
pub mod scans;
pub mod templates;
pub mod textdoc;
pub(crate) mod typeset;
