# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/zeon256/satay-rs/releases/tag/satay-codegen-v0.1.0) - 2026-06-03

### Added

- normalize schema
- completed schemaid threading
- add explicit SchemaId
- impl type validated x-satay
- infallible x-satay lowering
- ensure we dont traverse the AST twice
- first cut of 3.1 support
- add support for integer ranges
- infer integer primitives from OpenAPI bounds
- add rustdoc comments from description
- add as-bool for ints
- add as-bool
- support enum mapping
- nicer DX
- add x-satay.treat-error-as-none support for graceful error handling
- add x-satay.parse-as codegen support
- *(codegen)* add sans-io action builders
- *(codegen)* add optional reqwest client generation
- *(codegen)* generate named enum types for inline string enums with unknown fallback
- *(codegen)* generate per-endpoint module directories instead of monolithic file
- *(codegen)* supoprt header paramteres in OpenAPI operations
- *(codegen)* support pattern
- initial commit

### Fixed

- generated response name collisions
- ensure that all description will generate docs
- remove manual string manipulation
- ensure that u32 dont use as.ref
- ensure that generated nutype strings have Hash
- ensure that if it >0 auto deduction use unsigned int
- *(codegen)* ensure that there are newlines

### Other

- *(codegen)* address generated-crate check review feedback
- *(codegen)* expand seam coverage and harden generated crate checks
- remove prefixes in macros
- clippy lint
- dont use Vec::new()
- complete lowering
- make sure that TypeRegistry::finish takes in Vec<Component>
- component lookup optimization
- iterate over tables instead of repeating near-identical calls
- impl typed validation IR
- cleanup constraint validation
- moved validation like checks from lower
- clippy lints
- make sure that x-satay is validated
- make it more like a compiler and decouple things
- split render into multiple files
- update cargo dep
- split BusStopCode into its own type
- split docs
- format files
- dylint lints
- [**breaking**] replace from_raw_parts with ResponseParts in decode signatures
- *(codegen)* add blank lines
- *(codegen)* add blank lines between struct/enum fields and import std::fmt
- *(codegen)* dylint lints
- add tracing to all crates
- *(codegen)* dylint lints for tests
- *(codegen)* error enum variants
- *(parser)* add unit tests
- *(codegen)* split error enum into parse and validation errors
- *(codegen)* build render output with syn AST
- split monolith into smaller files
- use thiserror instead of stringly typed errors
