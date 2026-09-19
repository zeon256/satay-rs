use super::generated::*;

#[test]
fn tagged_union_deserializes_response() {
    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: br#"{"kind":"cat","name":"Milo","lives":9}"#.to_vec(),
    };

    let decoded: GetPetResponse = operations::get_pet::decode_get_pet_response(response.as_bytes())
        .expect("decoded response");
    match decoded {
        GetPetResponse::Ok(Pet::Cat(value)) => {
            assert_eq!(value.name, "Milo");
            assert_eq!(value.lives, 9);
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn tagged_union_serializes_tag() {
    let value = Pet::Dog(Dog {
        name: "Rex".to_owned(),
        bark_volume: 7,
    });
    let encoded = serde_json::to_value(value).expect("serialized pet");
    assert_eq!(
        encoded,
        serde_json::json!({
            "kind": "dog",
            "name": "Rex",
            "barkVolume": 7
        })
    );
}
