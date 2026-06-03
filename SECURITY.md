# Security Policy

## Supported Versions

Security fixes are provided for the latest published versions of the Satay crates:

- `satay-cli` (the `satay` executable)
- `satay-codegen`
- `satay-runtime`
- `satay-reqwest`
- `satay-ureq`

Until the project reaches `1.0`, compatibility may change between minor versions. Please test against the latest release before reporting an issue that may already be fixed.

## Reporting a Vulnerability

Please do not open a public issue for a suspected security vulnerability.

Report privately by contacting the maintainer through the [repository owner account](https://github.com/zeon256) or the contact method listed on [crates.io](https://crates.io/crates/satay-cli), if available. Include:

- affected crate, version, or commit
- a minimal reproducer (OpenAPI document, CLI invocation, or generated-code example as appropriate)
- expected behavior
- observed behavior
- impact assessment

You should receive an initial response within 7 days. If the report is accepted, a fix and advisory will be coordinated before public disclosure where practical.

## Security Scope

Issues generally considered security-sensitive include:

- panics, hangs, or excessive resource usage triggered by untrusted OpenAPI or YAML input during parsing or code generation
- path traversal or unintended file overwrite when using the `satay` CLI with attacker-controlled `--input` or `--output` paths
- unsound Rust or memory safety issues in Satay libraries or generated code templates
- generated request builders or decoders that omit required auth, mis-handle secrets, or construct unsafe URLs or headers from spec data
- validation newtypes or decoders that accept values outside declared OpenAPI constraints in ways that could affect authorization or integrity checks

Issues generally not considered vulnerabilities by themselves:

- bugs in a remote API that do not match its OpenAPI document
- application misuse of generated clients or transport adapters
- missing support for OpenAPI features unless they cause incorrect, unsafe, or exploitable generated code
- differences between ECMA-262 `pattern` regexes in a spec and Rust `regex` semantics, when the mismatch is documented and does not bypass stated constraints

## Disclosure

Accepted vulnerabilities will be fixed in a patch release when possible. Public disclosure should include the affected versions, fixed versions, impact, and suggested mitigation.

## Build & Supply Chain Hardening

This project uses the following hardening techniques:

- **Trusted Publishing**: crates.io releases are published via GitHub Actions OIDC, eliminating the need for long-lived API tokens.
- **Pinned SHA**: all GitHub Actions dependencies reference specific commit SHAs rather than mutable version tags, mitigating supply chain attacks.
