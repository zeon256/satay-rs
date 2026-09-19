use super::generated::{GetIndicatorsInput, Indicators, operations};
use serde_json::{Value, json};

fn value_with(strict: Value, fallback: Value) -> Value {
    json!({
        "strict": strict,
        "fallback": fallback,
        "lossy": "Y",
        "requiredNullable": "Y",
        "noneMapped": "Y"
    })
}

#[test]
fn configured_values_decode_and_use_first_values_for_serialization() {
    for value in ["Y", "Yes", "1", "true"] {
        let decoded: Indicators =
            serde_json::from_value(value_with(json!(value), json!("Y"))).unwrap();
        assert!(decoded.strict, "{value}");
    }
    for value in ["N", "No", "0", "false", ""] {
        let decoded: Indicators =
            serde_json::from_value(value_with(json!(value), json!("Y"))).unwrap();
        assert!(!decoded.strict, "{value}");
    }

    let true_parts =
        operations::get_indicators::get_indicators_parts(GetIndicatorsInput::new(true)).unwrap();
    assert_eq!(true_parts.uri, "/indicators?enabled=Y");
    let false_parts =
        operations::get_indicators::get_indicators_parts(GetIndicatorsInput::new(false)).unwrap();
    assert_eq!(false_parts.uri, "/indicators?enabled=N");

    let encoded = serde_json::to_value(Indicators {
        strict: true,
        fallback: false,
        lossy: None,
        optional: None,
        reusable: Some(true),
        required_nullable: Some(false),
        none_mapped: None,
    })
    .unwrap();
    assert_eq!(
        encoded,
        json!({
            "strict": "Y",
            "fallback": "N",
            "requiredNullable": "N",
            "reusable": "Y",
            "noneMapped": ""
        })
    );
}

#[test]
fn unknown_values_are_strict_unless_a_fallback_is_configured() {
    let fallback: Indicators =
        serde_json::from_value(value_with(json!("Y"), json!("upstream-drift"))).unwrap();
    assert!(!fallback.fallback);

    let lossy: Indicators = serde_json::from_value(json!({
        "strict": "Y",
        "fallback": "N",
        "lossy": "upstream-drift",
        "requiredNullable": "Y",
        "noneMapped": "Y"
    }))
    .unwrap();
    assert_eq!(lossy.lossy, None);

    assert!(
        serde_json::from_value::<Indicators>(value_with(json!("unknown"), json!("Y"))).is_err()
    );
    assert!(serde_json::from_value::<Indicators>(value_with(json!("yes"), json!("Y"))).is_err());
    assert!(serde_json::from_value::<Indicators>(value_with(json!(2), json!("Y"))).is_err());

    let numeric: Indicators = serde_json::from_value(value_with(json!(1), json!("Y"))).unwrap();
    assert!(numeric.strict);
    let boolean: Indicators = serde_json::from_value(value_with(json!(false), json!("Y"))).unwrap();
    assert!(!boolean.strict);
}

#[test]
fn nullable_and_none_if_fields_preserve_their_distinct_contracts() {
    let decoded: Indicators = serde_json::from_value(json!({
        "strict": "Y",
        "fallback": "N",
        "optional": null,
        "requiredNullable": null,
        "noneMapped": ""
    }))
    .unwrap();
    assert_eq!(decoded.optional, None);
    assert_eq!(decoded.required_nullable, None);
    assert_eq!(decoded.none_mapped, None);

    let missing_optional: Indicators = serde_json::from_value(json!({
        "strict": "Y",
        "fallback": "N",
        "requiredNullable": "N",
        "noneMapped": "N"
    }))
    .unwrap();
    assert_eq!(missing_optional.optional, None);
    assert_eq!(missing_optional.required_nullable, Some(false));
    assert_eq!(missing_optional.none_mapped, Some(false));

    assert!(
        serde_json::from_value::<Indicators>(json!({
            "strict": null,
            "fallback": "N",
            "requiredNullable": "Y",
            "noneMapped": "Y"
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<Indicators>(json!({
            "strict": "Y",
            "fallback": "N",
            "noneMapped": "Y"
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<Indicators>(json!({
            "strict": "Y",
            "fallback": "N",
            "requiredNullable": "Y",
            "noneMapped": null
        }))
        .is_err()
    );
}
