# satay-reqwest

Send generated Satay actions with `reqwest`.

This crate provides the `ReqwestActionExt` trait, which adds `send_with` to any generated action for async `reqwest`. Enable the `blocking` feature for `ReqwestBlockingActionExt` with `reqwest::blocking`.

```rust
use satay_reqwest::ReqwestActionExt;

let client = reqwest::Client::new();
let response = action.send_with(&client).await?;
```

`reqwest` is re-exported so downstream crates can use the same version without adding their own dependency.

If you are looking for `satay-rs`, visit the [main repository](https://github.com/zeon256/satay-rs).
