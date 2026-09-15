use satay_ir::{IntegerInterpretation, PropertyPolicy, StringInterpretation, TypeExpr};

use crate::parse::tests::{INLINE_CONSTRAINED_ENUM_RANGE, parse_valid};

use super::{definition, normalize, object, string};

fn fixture(spec: &str) -> satay_ir::Api {
    // The legacy parser is an independent oracle, never a normalization prerequisite.
    parse_valid(spec);
    normalize(spec)
}

#[test]
fn simple_preserves_shared_status_reference_and_http_parameters() {
    let api = fixture(include_str!("../../../../../../tests/fixtures/simple.yaml"));
    let user = object(&definition(&api, "User").schema);
    let status = user
        .properties
        .iter()
        .find(|p| p.wire_name == "status")
        .unwrap();

    let TypeExpr::Ref(id) = status.value.ty else {
        panic!("shared status definition")
    };

    assert_eq!(api.definition(id).unwrap().source_name, "UserStatus");
    assert_eq!(
        string(&api.definition(id).unwrap().schema)
            .enum_values
            .as_deref(),
        Some(["active".to_owned(), "suspended".to_owned()].as_slice())
    );

    let path = &api.http().paths[0];
    assert_eq!(path.parameters[0].wire_name, "userId");
    assert_eq!(path.operations[0].parameters[0].wire_name, "includeDetails");
    assert_eq!(
        path.operations[1].request_body.as_ref().unwrap().content[0].media_type,
        "application/vnd.satay.user+json"
    );
    assert_eq!(
        api.http().tags[0].description.as_deref(),
        Some("User management operations.")
    );
}

#[test]
fn petstore_retains_array_items_and_declared_operation_ids() {
    let api = fixture(include_str!(
        "../../../../../../tests/fixtures/petstore-minimal.yaml"
    ));

    let operation = &api.http().paths[0].operations[0];
    assert_eq!(operation.source_id.as_deref(), Some("listPets"));

    let TypeExpr::Array(array) = &operation.responses[0].content[0]
        .media
        .schema
        .as_ref()
        .unwrap()
        .ty
    else {
        panic!("array response")
    };

    let TypeExpr::Ref(id) = array.items.ty else {
        panic!("pet identity")
    };

    assert_eq!(api.definition(id).unwrap().source_name, "Pet");
    let pet = object(&api.definition(id).unwrap().schema);
    assert_eq!(
        pet.properties
            .iter()
            .map(|p| (p.wire_name.as_str(), p.required))
            .collect::<Vec<_>>(),
        [("id", true), ("name", true), ("tag", false)]
    );
}

#[test]
fn constrained_retains_bounds_nullable_requiredness_and_formats() {
    let api = fixture(include_str!(
        "../../../../../../tests/fixtures/constrained.yaml"
    ));

    let TypeExpr::Integer(age) = &definition(&api, "Age").schema.ty else {
        panic!("integer age")
    };

    assert_eq!(age.constraints.minimum.as_ref().unwrap().value, 0.into());
    assert_eq!(age.constraints.maximum.as_ref().unwrap().value, 130.into());
    assert_eq!(
        age.interpretation,
        IntegerInterpretation::Numeric {
            representation: None
        }
    );

    let user = object(&definition(&api, "User").schema);
    let nickname = user
        .properties
        .iter()
        .find(|p| p.wire_name == "nickname")
        .unwrap();

    assert!(nickname.required && nickname.value.nullable);

    let score = &user
        .properties
        .iter()
        .find(|p| p.wire_name == "score")
        .unwrap()
        .value;

    assert_eq!(score.annotations.format.as_deref(), Some("float"));

    let TypeExpr::Number(number) = &score.ty else {
        panic!("number score")
    };
    assert!(number.constraints.minimum.as_ref().unwrap().exclusive);
    assert!(number.constraints.maximum.as_ref().unwrap().exclusive);
}

#[test]
fn inline_enums_remain_inline_without_synthetic_definitions() {
    let api = fixture(include_str!(
        "../../../../../../tests/fixtures/inline-enum.yaml"
    ));

    assert_eq!(
        api.definitions()
            .map(|(_, d)| d.source_name.as_str())
            .collect::<Vec<_>>(),
        ["Item"]
    );

    let item = object(&definition(&api, "Item").schema);

    assert_eq!(
        string(&item.properties[2].value)
            .enum_values
            .as_ref()
            .unwrap(),
        &["electronics", "clothing", "food"]
    );
    assert_eq!(
        string(&item.properties[3].value)
            .enum_values
            .as_ref()
            .unwrap(),
        &["new", "used", "refurbished", ""]
    );
}

#[test]
fn property_identifiers_preserve_wire_names_and_requested_words() {
    let api = fixture(include_str!(
        "../../../../../../tests/fixtures/property-identifiers.yaml"
    ));

    let stop = object(&definition(&api, "BusStop").schema);
    let request = stop
        .properties
        .iter()
        .find(|p| p.wire_name == "RequestIdentifier")
        .unwrap();

    let PropertyPolicy::Included { identifier, .. } = &request.policy else {
        panic!("included")
    };

    assert_eq!(identifier.as_ref().unwrap(), &["request", "id"]);
    assert!(request.required);

    let lat = stop
        .properties
        .iter()
        .find(|p| p.wire_name == "Latitude")
        .unwrap();

    assert_eq!(lat.value.annotations.format.as_deref(), Some("double"));
}

#[test]
fn inline_range_fixture_retains_range_scalar_bounds() {
    let api = fixture(INLINE_CONSTRAINED_ENUM_RANGE);
    let search = object(&definition(&api, "Search").schema);

    assert_eq!(
        string(&search.properties[1].value)
            .enum_values
            .as_ref()
            .unwrap(),
        &["open", "closed"]
    );

    let StringInterpretation::IntegerRange {
        representation,
        bounds,
    } = &string(&search.properties[2].value).interpretation
    else {
        panic!("integer range")
    };

    assert_eq!(*representation, None);
    assert_eq!(bounds.minimum.as_ref().unwrap().value, 1.into());
    assert_eq!(bounds.maximum.as_ref().unwrap().value, 60.into());
}
