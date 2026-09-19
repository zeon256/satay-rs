use super::{BoolStringMapping, BoolStringMappingError};

#[test]
fn bool_string_mapping_preserves_values_and_unknown_as() {
    let mapping = BoolStringMapping::try_new(
        vec!["yes".to_owned(), "true".to_owned(), "yes".to_owned()],
        vec!["no".to_owned(), "false".to_owned(), "no".to_owned()],
        Some(true),
    )
    .expect("mapping should be valid");

    assert_eq!(
        mapping.true_values(),
        ["yes".to_owned(), "true".to_owned(), "yes".to_owned()]
    );
    assert_eq!(
        mapping.false_values(),
        ["no".to_owned(), "false".to_owned(), "no".to_owned()]
    );
    assert_eq!(mapping.unknown_as(), Some(true));
}

#[test]
fn bool_string_mapping_rejects_empty_true_values() {
    assert_eq!(
        BoolStringMapping::try_new(vec![], vec!["no".to_owned()], None),
        Err(BoolStringMappingError::EmptyTrueValues)
    );
}

#[test]
fn bool_string_mapping_rejects_empty_false_values() {
    assert_eq!(
        BoolStringMapping::try_new(vec!["yes".to_owned()], vec![], None),
        Err(BoolStringMappingError::EmptyFalseValues)
    );
}

#[test]
fn bool_string_mapping_reports_the_first_overlap_in_false_value_order() {
    assert_eq!(
        BoolStringMapping::try_new(
            vec!["first-in-true".to_owned(), "first-in-false".to_owned()],
            vec!["first-in-false".to_owned(), "first-in-true".to_owned()],
            Some(false),
        ),
        Err(BoolStringMappingError::OverlappingValue(
            "first-in-false".to_owned()
        ))
    );
}

#[test]
fn bool_string_mapping_preserves_none_unknown_as() {
    let mapping = BoolStringMapping::try_new(vec!["yes".to_owned()], vec!["no".to_owned()], None)
        .expect("mapping should be valid");

    assert_eq!(mapping.unknown_as(), None);
}
