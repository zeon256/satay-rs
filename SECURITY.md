# Security Policy

## Supported Versions

Security fixes are provided for the latest published version of `fast-robots`.

Until the crate reaches `1.0`, compatibility may change between minor versions. Please test against the latest release before reporting an issue that may already be fixed.

## Reporting a Vulnerability

Please do not open a public issue for a suspected security vulnerability.

Report privately by contacting the maintainer through the repository owner account or the contact method listed on crates.io, if available. Include:

- affected version or commit
- a minimal reproducer
- expected behavior
- observed behavior
- impact assessment

You should receive an initial response within 7 days. If the report is accepted, a fix and advisory will be coordinated before public disclosure where practical.

## Security Scope

Issues generally considered security-sensitive include:

- panics or excessive resource usage triggered by untrusted `robots.txt` input
- incorrect allow/disallow decisions with clear crawler policy impact
- unsound Rust or memory safety issues
- CLI behavior that can overwrite, delete, or unexpectedly expose local data

Issues generally not considered vulnerabilities by themselves:

- a website using `robots.txt` as an authorization mechanism
- crawler-specific differences for extension directives such as `Crawl-delay`
- missing support for non-standard directives unless they affect RFC 9309 parsing

## Disclosure

Accepted vulnerabilities will be fixed in a patch release when possible. Public disclosure should include the affected versions, fixed versions, impact, and suggested mitigation.

## Build & Supply Chain Hardening

This project uses the following hardening techniques:

- **Trusted Publishing**: crates.io releases are published via GitHub Actions OIDC, eliminating the need for long-lived API tokens.
- **Pinned SHA**: all GitHub Actions dependencies reference specific commit SHAs rather than mutable version tags, mitigating supply chain attacks.
