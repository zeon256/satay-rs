#![allow(
    clippy::doc_markdown,
    clippy::needless_raw_string_hashes,
    clippy::too_many_lines
)]

//! End-to-end tests generating, compiling, and exercising client crates.

mod all_of;
mod ast;
mod behavior;
mod common;
mod coordinates;
mod enums;
mod identifiers;
mod ignore;
mod integers;
mod maps;
mod parameters;
mod parse_as;
mod rejections;
mod responses;
mod roots;
mod storage;
mod structure;
mod unions;
mod urls;

use satay_codegen as codegen;
