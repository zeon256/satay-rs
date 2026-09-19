use super::*;

fn integer(value: i64) -> Number {
    Number::from(value)
}

fn unsigned(value: u64) -> Number {
    Number::from(value)
}

fn float(value: f64) -> Number {
    Number::from_f64(value).expect("finite JSON number")
}

fn validation_error<T>(result: Result<T, ValidationError>) -> ValidationError {
    match result {
        Ok(_) => panic!("expected validation error"),
        Err(error) => error,
    }
}

#[test]
fn parses_string_validation_keywords() {
    let schema = ConstraintInput {
        min_length: Some(2),
        max_length: Some(16),
        pattern: Some("^[a-z]+$".to_owned()),
        ..ConstraintInput::default()
    };

    let validation = parse_validation(&schema, &TypeRef::String, "User.name").unwrap();

    assert_eq!(
        validation,
        Some(Validation::String {
            min_length: Some(2),
            max_length: Some(16),
            pattern: Some("^[a-z]+$".to_owned()),
        })
    );
}

#[test]
fn rejects_invalid_string_length_bounds() {
    let schema = ConstraintInput {
        min_length: Some(17),
        max_length: Some(16),
        ..ConstraintInput::default()
    };

    let error = validation_error(parse_validation(&schema, &TypeRef::String, "User.name"));

    assert!(matches!(
        error,
        ValidationError::InvalidStringLengthBounds {
            context,
            min_length: 17,
            max_length: 16,
        } if context == "User.name"
    ));
}

#[test]
fn infers_smallest_integer_type_from_bounded_range() {
    let unsigned_byte_schema = ConstraintInput {
        minimum: Some(unsigned(0)),
        maximum: Some(unsigned(255)),
        ..ConstraintInput::default()
    };
    let signed_byte_schema = ConstraintInput {
        minimum: Some(integer(-128)),
        maximum: Some(integer(127)),
        ..ConstraintInput::default()
    };

    assert_eq!(
        parse_integer_type(&unsigned_byte_schema, "Byte", None).unwrap(),
        IntegerType::U8
    );
    assert_eq!(
        parse_integer_type(&signed_byte_schema, "SignedByte", None).unwrap(),
        IntegerType::I8
    );
}

#[test]
fn infers_unsigned_integer_type_from_non_negative_minimum() {
    let schema = ConstraintInput {
        minimum: Some(unsigned(0)),
        ..ConstraintInput::default()
    };

    assert_eq!(
        parse_integer_type(&schema, "User.count", None).unwrap(),
        IntegerType::U64
    );
}

#[test]
fn parses_integer_validation_with_tighter_exclusive_limits() {
    let schema = ConstraintInput {
        minimum: Some(integer(0)),
        exclusive_minimum: Some(integer(4)),
        maximum: Some(integer(10)),
        exclusive_maximum: Some(integer(10)),
        ..ConstraintInput::default()
    };

    let validation =
        parse_validation(&schema, &TypeRef::Integer(IntegerType::I64), "User.score").unwrap();

    assert_eq!(
        validation,
        Some(Validation::Integer {
            minimum: Some(IntegerLimit {
                value: 4,
                exclusive: true,
            }),
            maximum: Some(IntegerLimit {
                value: 10,
                exclusive: true,
            }),
        })
    );
}

#[test]
fn rejects_integer_bounds_outside_integer_type_range() {
    let schema = ConstraintInput {
        minimum: Some(unsigned(300)),
        maximum: Some(unsigned(400)),
        ..ConstraintInput::default()
    };

    let error = validation_error(parse_validation(
        &schema,
        &TypeRef::Integer(IntegerType::U8),
        "User.age",
    ));

    assert!(matches!(
        error,
        ValidationError::EmptyIntegerBounds { context } if context == "User.age"
    ));
}

#[test]
fn rejects_multiple_of_for_integer_validation() {
    let schema = ConstraintInput {
        multiple_of: Some(unsigned(2)),
        ..ConstraintInput::default()
    };

    let error = validation_error(parse_validation(
        &schema,
        &TypeRef::Integer(IntegerType::I64),
        "User.count",
    ));

    assert!(matches!(
        error,
        ValidationError::UnsupportedKeyword {
            context,
            keyword,
        } if context == "User.count" && keyword == "multipleOf"
    ));
}

#[test]
fn parses_number_validation_with_tighter_exclusive_limits() {
    let schema = ConstraintInput {
        minimum: Some(float(0.0)),
        exclusive_minimum: Some(float(1.5)),
        maximum: Some(float(10.0)),
        exclusive_maximum: Some(float(9.5)),
        ..ConstraintInput::default()
    };

    let validation = parse_validation(&schema, &TypeRef::F64, "User.ratio").unwrap();

    assert_eq!(
        validation,
        Some(Validation::Number {
            minimum: Some(FloatLimit {
                value: 1.5,
                exclusive: true,
            }),
            maximum: Some(FloatLimit {
                value: 9.5,
                exclusive: true,
            }),
        })
    );
}

#[test]
fn rejects_invalid_array_length_bounds() {
    let schema = ConstraintInput {
        min_items: Some(3),
        max_items: Some(2),
        ..ConstraintInput::default()
    };

    let error = validation_error(parse_validation(
        &schema,
        &TypeRef::Array(Box::new(TypeRef::String)),
        "User.tags",
    ));

    assert!(matches!(
        error,
        ValidationError::InvalidArrayLengthBounds {
            context,
            min_items: 3,
            max_items: 2,
        } if context == "User.tags"
    ));
}
