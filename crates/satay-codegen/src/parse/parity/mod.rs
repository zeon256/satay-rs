//! Runs the facade's unchanged assertions against semantic generation.
#![allow(
    clippy::doc_markdown,
    clippy::needless_raw_string_hashes,
    clippy::too_many_lines
)]
#[path = "../../../tests/generate/all_of.rs"]
mod all_of;
#[path = "../../../tests/generate/ast.rs"]
mod ast;
#[path = "../../../tests/generate/behavior.rs"]
mod behavior;
pub(in crate::parse) mod codegen;
#[path = "../../../tests/generate/common.rs"]
mod common;
#[path = "../../../tests/generate/coordinates.rs"]
mod coordinates;
#[path = "../../../tests/generate/enums.rs"]
mod enums;
#[path = "../../../tests/generate/identifiers.rs"]
mod identifiers;
#[path = "../../../tests/generate/ignore.rs"]
mod ignore;
#[path = "../../../tests/generate/integers.rs"]
mod integers;
#[path = "../../../tests/generate/maps.rs"]
mod maps;
#[path = "../../../tests/generate/parameters.rs"]
mod parameters;
#[path = "../../../tests/generate/parse_as.rs"]
mod parse_as;
mod regressions;
#[path = "../../../tests/generate/rejections.rs"]
mod rejections;
#[path = "../../../tests/generate/responses.rs"]
mod responses;
mod roots;
mod semantics;
#[path = "../../../tests/generate/storage.rs"]
mod storage;
#[path = "../../../tests/generate/structure.rs"]
mod structure;
#[path = "../../../tests/generate/unions.rs"]
mod unions;
#[path = "../../../tests/generate/urls.rs"]
mod urls;

pub(in crate::parse) fn assert_generation(spec: &str) {
    let _ = codegen::generate(spec);
}
