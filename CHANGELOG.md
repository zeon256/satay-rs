# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.3.0...satay-codegen-v0.4.0) - 2026-06-11

### Added

- *(codegen)* generate named structs for inline object allOf
- *(codegen)* support open and inline string enums
- *(codegen)* support inline singleton union branches
- *(codegen)* support ref-only oneOf unions

### Fixed

- *(codegen)* reject shadowed plain union branches
- *(codegen)* ensure that big enum variant is boxed
- *(codegen)* support inline primitive oneOf branches
- *(codegen)* omit redundant serde rename for raw identifiers
- *(codegen)* support embedded discriminator fields
- *(codegen)* thread recursion stack through discriminator branches
- *(codegen)* reject recursive inline allOf
- *(codegen)* preserve open enum branch descriptions
- *(codegen)* make open string enum detection exhaustive
- *(codegen)* restore const as_str for closed enums
- *(codegen)* reserve Other only for open enum fallback
- *(codegen)* allow Unknown as an enum variant
- *(codegen)* allow vendor metadata on union schemas

### Other

- *(codegen)* constrained string union branches parse instead of erroring
- *(codegen)* ensure that code is formatted properly
- *(codegen)* ensure dylint lints passes
- *(codegen)* make sure that the test use syn instead of checking strings
- *(codegen)* split rejects_unsupported_openapi_31_schema_forms_explicitly
- *(codegen)* split tests and functionality

## [0.3.0](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.2.0...satay-codegen-v0.3.0) - 2026-06-09

### Added

- *(codegen)* discriminator union support for oneOf
- *(codegen)* add initial support for allOf

### Fixed

- *(codegen)* apply implicit discriminator mapping defaults

### Other

- *(codegen)* add docs for how much allOf is supported
- *(codegen)* add more negative test caes

## [0.2.0](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.1.4...satay-codegen-v0.2.0) - 2026-06-09

### Added

- *(codegen)* make sure datetimes dont get additional reference
- *(codegen)* support for unixtime
- *(codegen)* reject empty anyOf unions and alias-indirected cycles
- *(codegen)* add support for local schema anyOf

### Other

- *(codegen)* make sure to document big branch

## [0.1.4](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.1.3...satay-codegen-v0.1.4) - 2026-06-05

### Other

- Revert "fix(codegen): parse i64 minimum properly"

## [0.1.3](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.1.2...satay-codegen-v0.1.3) - 2026-06-05

### Fixed

- *(codegen)* parse i64 minimum properly

### Other

- add msrv in the cargo

## [0.1.2](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.1.1...satay-codegen-v0.1.2) - 2026-06-04

### Fixed

- *(codegen)* ensure that we don't generate code that needlessly borrow
- ensure that format date takes dont needlessly pass value that is immediately deref

### Other

- *(codegen)* update ui test for naivedatetime
- update ui test for generating formatted date

## [0.1.1](https://github.com/zeon256/satay-rs/compare/satay-cli-v0.1.0...satay-cli-v0.1.1) - 2026-06-04

### Other

- update Cargo.toml dependencies
