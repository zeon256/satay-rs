# satay-ureq

Send generated Satay actions with `ureq`.

This crate provides the `UreqActionExt` trait, which adds `send_with` to any generated action for blocking HTTP via `ureq`.

```rust
use satay_ureq::UreqActionExt;

let agent = ureq::Agent::new();
let response = action.send_with(&agent)?;
```

`ureq` is re-exported so downstream crates can use the same version without adding their own dependency.

If you are looking for `satay-rs`, visit the [main repository](https://github.com/zeon256/satay-rs).
