//! The text of the pages: the positioned text layer, search, spell checking, operations on text ranges, editing the text, and the content-stream lexer and editor under them.

pub(crate) mod content_edit;
pub(crate) mod content_lex;
pub mod search;
pub mod spelling;
pub mod text;
pub mod textedit;
pub mod textlayer;
pub mod textops;
