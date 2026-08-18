# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]


### Fixed

- *(codegen)* import runtime serde modules in generated types instead of emitting fully qualified call-site and serde attribute paths, so the `minimal_imports` lint passes in generated clients

## [0.16.0](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.15.3...satay-codegen-v0.16.0) - 2026-08-17

### Added

- *(runtime)* configure boolean string mappings

### Added

- *(codegen)* configure canonical true and false strings for string-backed boolean fields

## [0.15.3](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.15.2...satay-codegen-v0.15.3) - 2026-08-17

### Fixed

- generated none-if serializers fail pedantic Clippy for small Copy types

### Fixed

- *(codegen)* scope lint allowances in generated none-if serializers so pedantic Clippy passes on small `Copy` field types

## [0.15.2](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.15.1...satay-codegen-v0.15.2) - 2026-08-13

### Fixed

- *(codegen)* dont emit serde import in in open enum

## [0.15.1](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.15.0...satay-codegen-v0.15.1) - 2026-08-13

### Fixed

- *(codegen)* emit unqualified Deserialize in open enum

## [0.15.0](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.14.0...satay-codegen-v0.15.0) - 2026-08-07

### Added

- *(codegen)* support property identifier overrides

## [0.14.0](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.13.0...satay-codegen-v0.14.0) - 2026-08-07

### Added

- *(codegen)* ignore wire-only object properties

## [0.13.0](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.12.2...satay-codegen-v0.13.0) - 2026-08-07

### Added

- *(codegen)* support response projection

### Other

- *(codegen)* centralize x-satay wire contracts

## [0.12.2](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.12.1...satay-codegen-v0.12.2) - 2026-08-06

### Added

- *(codegen)* make sure that imports for group APIs dont have >= 3 segments

### Fixed

- *(codegen)* avoid action constructor parameter collisions

## [0.12.1](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.12.0...satay-codegen-v0.12.1) - 2026-08-05

### Added

- *(codegen)* clarify generated action builders
- *(codegen)* generate tag-grouped API namespaces

### Added

- *(codegen)* generate namespaced API views from OpenAPI operation tags and organize low-level helpers under `operations`
- *(codegen)* document consuming action builders, optional setters, and usage examples in generated Rustdoc

## [0.12.0](https://github.com/zeon256/satay-rs/compare/satay-oas3-v0.11.0...satay-oas3-v0.12.0) - 2026-08-05

### Added

- *(oas3)* add borrowed component resolver ([#99](https://github.com/zeon256/satay-rs/pull/99))
- *(oas3)* parse typed local component references ([#98](https://github.com/zeon256/satay-rs/pull/98))
- *(oas3)* preserve unknown schema keywords ([#103](https://github.com/zeon256/satay-rs/pull/103))
- *(oas3)* centralize Schema traversal ([#101](https://github.com/zeon256/satay-rs/pull/101))
- *(oas3)* add typed specification extension access ([#102](https://github.com/zeon256/satay-rs/pull/102))
- *(oas3)* add Schema inspection helpers ([#100](https://github.com/zeon256/satay-rs/pull/100))

## [0.11.0](https://github.com/zeon256/satay-rs/compare/satay-oas3-v0.10.0...satay-oas3-v0.11.0) - 2026-08-04

### Other

- update version number for oas so that it matches us
- *(parser)* fork oas3 as satay-oas3 ([#91](https://github.com/zeon256/satay-rs/pull/91))

### Changed

- *(parser)* fork `oas3` as the in-tree `satay-oas3` crate and parse YAML with `serde-saphyr`

### Fixed

- *(codegen)* preserve `x-satay.treat-error-as-none` beside schema `$ref` fields

## [0.10.0](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.9.0...satay-codegen-v0.10.0) - 2026-08-03

### Added

- *(codegen)* accept nullable optional query and header parameters
- *(codegen)* add x-satay skip operation extension

## [0.9.0](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.8.1...satay-codegen-v0.9.0) - 2026-07-29

### Added

- *(codegen)* support sentinel values with parse-as
- *(codegen)* support wildcard status-code ranges in responses
- *(codegen)* support propertyless object components as map aliases
- *(codegen)* support uint32 and uint64 integer formats
- *(codegen)* ignore additionalProperties on allOf branches with properties
- *(codegen)* allow type object sibling on discriminator unions
- *(codegen)* ignore discriminator on plain object schemas

## [0.8.1](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.8.0...satay-codegen-v0.8.1) - 2026-07-05

### Added

- *(codegen)* unwrap annotation-only allOf wrappers around a single $ref

## [0.8.0](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.7.0...satay-codegen-v0.8.0) - 2026-07-05

### Added

- *(codegen)* support bare const string branches in anyOf as open string enums

## [0.7.0](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.6.0...satay-codegen-v0.7.0) - 2026-07-05

### Added

- support additionalProperties map schemas and empty-schema JSON values

## [0.6.0](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.5.0...satay-codegen-v0.6.0) - 2026-07-04

### Added

- *(codegen)* support inline discriminated oneOf branches in anyOf/oneOf unions

### Fixed

- *(codegen)* reject recursive discriminator unions instead of overflowing the stack

## [0.5.0](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.4.1...satay-codegen-v0.5.0) - 2026-07-04

### Added

- *(codegen)* support OpenAPI 3.1 const string discriminator tags

## [0.4.1](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.4.0...satay-codegen-v0.4.1) - 2026-06-25

### Other

- adhere to a more strict set of clippy lints

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
