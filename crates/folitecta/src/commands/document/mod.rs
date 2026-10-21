//! The document and its pages: files, e-mail (the attachment dialog and its model), page organisation, pictures, the clean-up of scanned pages, barcodes, text recognition, sources of combined documents, comparison and page-mark macros.

pub(crate) mod barcodes;
pub(crate) mod compare;
pub(crate) mod convert;
pub(crate) mod doctools;
pub(crate) mod file;
pub(crate) mod fileops;
pub(crate) mod macros;
pub(crate) mod mail;
pub(crate) mod mail_draft;
pub(crate) mod ocr;
pub(crate) mod pages;
pub(crate) mod pictures;
pub(crate) mod print;
pub(crate) mod scans;
pub(crate) mod sources;
pub(crate) mod split;
