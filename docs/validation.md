# Generated Validation Newtypes

Satay creates named `nutype` wrappers for constrained component schemas, inline constrained object fields, and inline constrained operation parameters.

Generated code that represents OpenAPI validation constraints uses `nutype` newtypes. Add these dependencies to crates that compile constrained generated clients:

```toml
nutype = { version = "0.7", features = ["serde"] }
```

When the OpenAPI spec contains string `pattern` constraints, also add:

```toml
nutype = { version = "0.7", features = ["serde", "regex"] }
regex = "1"
```

OpenAPI `pattern` uses ECMA-262 regex syntax, while `nutype` uses Rust's `regex` crate. Most common patterns are compatible, but ECMA features like lookahead, lookbehind, and backreferences are not supported by the Rust `regex` engine and will cause a compile error in the generated code.

## Component Schemas

Constrained component schemas such as:

```yaml
Age:
  type: integer
  format: int32
  minimum: 0
  maximum: 130
Email:
  type: string
  pattern: '^[^@]+@[^@]+\.[^@]+$'
User:
  type: object
  required:
    - age
    - name
    - score
  properties:
    age:
      $ref: '#/components/schemas/Age'
    email:
      $ref: '#/components/schemas/Email'
    name:
      type: string
      minLength: 1
      maxLength: 80
    score:
      type: number
      format: float
      minimum: 0
      maximum: 1
```

generate Rust like:

```rust
#[nutype::nutype(
    validate(less_or_equal = 130),
    derive(
        Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, AsRef, Deref, TryFrom, Into, Display
    ),
    cfg_attr(feature = "serde", derive(Serialize, Deserialize))
)]
pub struct Age(u8);

#[nutype::nutype(
    validate(regex = "^[^@]+@[^@]+\\.[^@]+$"),
    derive(
        Debug, Clone, PartialEq, Eq, PartialOrd, Ord, AsRef, Deref, TryFrom, Into, Display
    ),
    cfg_attr(feature = "serde", derive(Serialize, Deserialize))
)]
pub struct Email(String);

#[nutype::nutype(
    validate(len_char_min = 1, len_char_max = 80),
    derive(
        Debug, Clone, PartialEq, Eq, PartialOrd, Ord, AsRef, Deref, TryFrom, Into, Display
    ),
    cfg_attr(feature = "serde", derive(Serialize, Deserialize))
)]
pub struct UserName(String);

#[nutype::nutype(
    validate(finite, greater_or_equal = 0.0, less_or_equal = 1.0),
    derive(Debug, Clone, Copy, PartialEq, PartialOrd, AsRef, Deref, TryFrom, Into, Display),
    cfg_attr(feature = "serde", derive(Serialize, Deserialize))
)]
pub struct UserScore(f32);

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct User {
    pub age: Age,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub email: Option<Email>,
    pub name: UserName,
    pub score: UserScore,
}
```

Integer schemas with both `minimum` and `maximum` infer the smallest Rust integer primitive that fits the declared range. In the `Age` example, `minimum: 0` is enforced by the `u8` backing type and the remaining `maximum: 130` constraint is enforced by `nutype`. Unformatted integer schemas with a one-sided non-negative lower bound and no `maximum` infer `u64`.

## Operation Parameters

Inline constrained operation parameters get operation-scoped names:

```rust
#[nutype::nutype(
    validate(regex = "^[a-zA-Z0-9-]+$", len_char_min = 3, len_char_max = 20),
    derive(
        Debug, Clone, PartialEq, Eq, PartialOrd, Ord, AsRef, Deref, TryFrom, Into, Display
    ),
    cfg_attr(feature = "serde", derive(Serialize, Deserialize))
)]
pub struct GetUserUserIdParameter(String);

#[nutype::nutype(
    validate(greater_or_equal = 1, less_or_equal = 100),
    derive(
        Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, AsRef, Deref, TryFrom, Into, Display
    ),
    cfg_attr(feature = "serde", derive(Serialize, Deserialize))
)]
pub struct GetUserLimitParameter(i32);

#[nutype::nutype(
    validate(len_char_min = 2),
    derive(
        Debug, Clone, PartialEq, Eq, PartialOrd, Ord, AsRef, Deref, TryFrom, Into, Display
    ),
    cfg_attr(feature = "serde", derive(Serialize, Deserialize))
)]
pub struct GetUserTagsParameterItem(String);

#[nutype::nutype(
    validate(predicate = |items| items.len() >= 1 && items.len() <= 3),
    derive(Debug, Clone, PartialEq, AsRef, Deref, TryFrom, Into),
    cfg_attr(feature = "serde", derive(Serialize, Deserialize))
)]
pub struct GetUserTagsParameter(Vec<GetUserTagsParameterItem>);
```

Call sites construct constrained values with `try_new` before building requests:

```rust
let user_id = GetUserUserIdParameter::try_new("user-42".to_owned()).expect("valid user id");
let limit = GetUserLimitParameter::try_new(10).expect("valid limit");
let tag = GetUserTagsParameterItem::try_new("rs".to_owned()).expect("valid tag");
let tags = GetUserTagsParameter::try_new(vec![tag]).expect("valid tag list");

let input = GetUserInput::new(user_id).limit(limit).tags(tags);

assert!(Age::try_new(131).is_err());
assert!(Email::try_new("not-an-email".to_owned()).is_err());
```

With the generated crate's `serde` feature enabled, response deserialization uses the same validators, so JSON payloads that violate OpenAPI bounds fail during decoding instead of producing invalid typed values.
