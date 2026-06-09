# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/zeon256/satay-rs/compare/satay-codegen-v0.1.4...satay-codegen-v0.2.0) - 2026-06-09

### Added

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
