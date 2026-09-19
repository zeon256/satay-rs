use super::generated::{Coordinates, Observation};
use serde_json::{Value, json};

fn wire() -> Value {
    json!({
        "Label": "car park",
        "Object": {"Latitude": 1.25, "Longitude": 103.5},
        "Aliased": "1.25,103.5",
        "Space": " 1.25 103.5 ",
        "Reversed": "103.5, 1.25",
        "Nullable": null,
        "Sentinel": "-",
        "Lossy": "1.25 103.5 1.5 104"
    })
}

#[test]
fn string_and_object_fields_share_the_same_validated_public_type() {
    let observation: Observation = serde_json::from_value(wire()).unwrap();
    let coordinates: &Coordinates = &observation.space;
    assert_eq!(coordinates, &observation.object);
    assert_eq!(coordinates, &observation.reversed);
    assert_eq!(coordinates, &observation.aliased);
    assert_eq!(*coordinates.lat.as_ref(), 1.25_f64);
    assert_eq!(*coordinates.long.as_ref(), 103.5_f32);
    assert_eq!(observation.nullable, None);
    assert_eq!(observation.optional, None);
    assert_eq!(observation.sentinel, None);
    assert_eq!(observation.lossy, None);

    let encoded = serde_json::to_value(&observation).unwrap();
    assert_eq!(
        encoded,
        json!({
            "Label": "car park",
            "Object": {"Latitude": 1.25, "Longitude": 103.5},
            "Aliased": "1.25,103.5",
            "Space": "1.25 103.5",
            "Reversed": "103.5,1.25",
            "Nullable": null,
            "Sentinel": ""
        })
    );
    let decoded: Observation = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, observation);
}

#[test]
fn primitive_fields_support_multicharacter_delimiters_and_reject_nonfinite_values() {
    use super::generated::{PlainCoordinates, PlainObservation};
    let observation: PlainObservation =
        serde_json::from_str(r#"{"position": "-1.25::2e-3"}"#).unwrap();
    assert_eq!(
        observation.position,
        PlainCoordinates {
            x: -1.25_f32,
            y: 0.002_f64
        }
    );
    assert_eq!(
        serde_json::to_value(&observation).unwrap(),
        json!({"position": "-1.25::0.002"})
    );
    let invalid = PlainObservation {
        position: PlainCoordinates {
            x: f32::INFINITY,
            y: 0.0,
        },
    };
    assert!(serde_json::to_value(invalid).is_err());
    for value in ["NaN::0", "0::inf", "1e100::0", "0::::1"] {
        assert!(serde_json::from_value::<PlainObservation>(json!({"position": value})).is_err());
    }
}

#[test]
fn numeric_constraints_and_exact_component_count_are_not_bypassed() {
    for invalid in [
        "91 103.5",
        "1.25 181",
        "1.25 103.5 1.5 104",
        "1.25",
        "1.25  103.5",
        "NaN 103.5",
        "1.25 inf",
        "1.25,103.5",
        "",
        "-",
    ] {
        let mut input = wire();
        input["Space"] = json!(invalid);
        assert!(
            serde_json::from_value::<Observation>(input).is_err(),
            "accepted {invalid:?}"
        );
    }
    let mut input = wire();
    input["Object"]["Latitude"] = json!(91);
    assert!(serde_json::from_value::<Observation>(input).is_err());
    let mut input = wire();
    input["Space"] = json!({"Latitude": 1.25, "Longitude": 103.5});
    assert!(serde_json::from_value::<Observation>(input).is_err());
}

#[test]
fn sentinel_lossy_and_missing_are_distinct_policies() {
    let mut input = wire();
    input["Sentinel"] = json!("1.25 103.5");
    input["Optional"] = json!("1.25 103.5");
    input["Nullable"] = json!("1.25 103.5");
    input["Lossy"] = json!("1.25 103.5");
    let observation: Observation = serde_json::from_value(input).unwrap();
    assert_eq!(observation.sentinel.as_ref(), Some(&observation.object));
    assert_eq!(observation.optional, observation.sentinel);
    assert_eq!(observation.nullable, observation.sentinel);
    assert_eq!(observation.lossy, observation.sentinel);
    let encoded = serde_json::to_value(observation).unwrap();
    assert_eq!(encoded["Sentinel"], "1.25 103.5");
    assert_eq!(encoded["Lossy"], "1.25 103.5");

    let mut input = wire();
    input["Sentinel"] = json!("broken");
    assert!(serde_json::from_value::<Observation>(input).is_err());
    let mut input = wire();
    input["Lossy"] = json!({"Latitude": 1.25, "Longitude": 103.5});
    let observation: Observation = serde_json::from_value(input).unwrap();
    assert_eq!(observation.lossy, None);
    let mut input = wire();
    input.as_object_mut().unwrap().remove("Space");
    assert!(serde_json::from_value::<Observation>(input).is_err());
}
