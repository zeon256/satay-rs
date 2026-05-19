# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/zeon256/satay-rs/releases/tag/satay-cli-v0.1.0) - 2026-05-19

### Added

- *(codegen)* add optional reqwest client generation
- *(codegen)* generate per-endpoint module directories instead of monolithic file
- initial commit

### Other

- split docs
- *(cli)* dylint lints
- add tracing to all crates
- *(codegen)* split error enum into parse and validation errors
- use thiserror instead of stringly typed errors
