//! Checked construction of interpretation value types.

use satay_ir::{
    ApiBuilder, BoolMapping, CoordinatesInterpretation, DefinitionId, IntegerRepresentation,
    InterpretationError, SentinelValues,
};

fn reserved_id() -> DefinitionId {
    ApiBuilder::new().reserve_definition()
}

#[test]
fn empty_coordinate_fields_report_their_index() {
    let target = reserved_id();
    let first =
        CoordinatesInterpretation::new(target, [String::new(), "latitude".into()], " ".into());
    assert_eq!(
        first,
        Err(InterpretationError::EmptyCoordinateField { index: 0 })
    );

    let second =
        CoordinatesInterpretation::new(target, ["longitude".into(), String::new()], " ".into());
    assert_eq!(
        second,
        Err(InterpretationError::EmptyCoordinateField { index: 1 })
    );
}

#[test]
fn duplicate_coordinate_fields_fail() {
    let target = reserved_id();
    let error =
        CoordinatesInterpretation::new(target, ["latitude".into(), "latitude".into()], " ".into());
    assert_eq!(error, Err(InterpretationError::DuplicateCoordinateField));
}

#[test]
fn empty_coordinate_delimiter_fails() {
    let target = reserved_id();
    let error = CoordinatesInterpretation::new(
        target,
        ["latitude".into(), "longitude".into()],
        String::new(),
    );
    assert_eq!(error, Err(InterpretationError::EmptyCoordinateDelimiter));
}

#[test]
fn successful_coordinates_retain_target_fields_and_delimiter() {
    let target = reserved_id();
    let coordinates =
        CoordinatesInterpretation::new(target, ["longitude".into(), "latitude".into()], ",".into())
            .unwrap();
    assert_eq!(coordinates.target(), target);
    assert_eq!(
        coordinates.fields(),
        &["longitude".to_string(), "latitude".to_string()]
    );
    assert_eq!(coordinates.delimiter(), ",");
}

#[test]
fn bool_mapping_requires_both_lists() {
    let error = BoolMapping::new(vec![], vec!["no".into()], None);
    assert_eq!(error, Err(InterpretationError::EmptyTrueValues));

    let error = BoolMapping::new(vec!["yes".into()], vec![], None);
    assert_eq!(error, Err(InterpretationError::EmptyFalseValues));
}

#[test]
fn overlapping_bool_values_fail_in_true_list_order() {
    // "maybe" appears in both lists; the true-list ordering determines which
    // overlap is reported first.
    let error = BoolMapping::new(
        vec!["yes".into(), "maybe".into()],
        vec!["no".into(), "maybe".into()],
        None,
    );
    assert_eq!(
        error,
        Err(InterpretationError::OverlappingBoolValue {
            value: "maybe".into()
        })
    );
}

#[test]
fn empty_string_is_a_valid_mapped_and_sentinel_value() {
    let mapping =
        BoolMapping::new(vec![String::new(), "yes".into()], vec!["no".into()], None).unwrap();
    assert_eq!(mapping.true_values(), &[String::new(), "yes".to_string()]);
    assert_eq!(mapping.false_values(), &["no".to_string()]);

    let sentinels = SentinelValues::new(vec![String::new(), "N/A".into()]).unwrap();
    assert_eq!(sentinels.values(), &[String::new(), "N/A".to_string()]);
}

#[test]
fn empty_sentinels_fail() {
    let error = SentinelValues::new(vec![]);
    assert_eq!(error, Err(InterpretationError::EmptySentinels));
}

#[test]
fn successful_mappings_retain_order_duplicates_and_unknown_policy() {
    let mapping = BoolMapping::new(
        vec!["yes".into(), "true".into(), "true".into()],
        vec!["no".into(), "false".into()],
        Some(false),
    )
    .unwrap();
    assert_eq!(
        mapping.true_values(),
        &["yes".to_string(), "true".to_string(), "true".to_string()]
    );
    assert_eq!(
        mapping.false_values(),
        &["no".to_string(), "false".to_string()]
    );
    assert_eq!(mapping.unknown_as(), Some(false));

    let sentinels = SentinelValues::new(vec!["N/A".into(), "N/A".into(), "-".into()]).unwrap();
    assert_eq!(
        sentinels.values(),
        &["N/A".to_string(), "N/A".to_string(), "-".to_string()]
    );
}

#[test]
fn representations_are_distinct_values() {
    assert_ne!(IntegerRepresentation::Auto, IntegerRepresentation::U32);
}
