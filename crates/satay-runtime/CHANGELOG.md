# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/zeon256/satay-rs/releases/tag/satay-runtime-v0.1.0) - 2026-05-16

### Added

- nicer DX
- add x-satay.treat-error-as-none support for graceful error handling
- add x-satay.parse-as codegen support
- *(codegen)* add sans-io action builders
- *(codegen)* add optional reqwest client generation
- *(runtime)* add nicer from_raw_parts
- *(codegen)* supoprt header paramteres in OpenAPI operations
- initial commit

### Other

- split docs
- dylint lints
- remove unnecessary qualifiers
- [**breaking**] replace from_raw_parts with ResponseParts in decode signatures
- *(codegen)* add blank lines
- add tracing to all crates
