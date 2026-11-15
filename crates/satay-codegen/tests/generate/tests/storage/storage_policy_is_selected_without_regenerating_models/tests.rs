use std::collections::BTreeMap;

use bumpalo::collections::{String as BumpString, Vec as BumpVec};
use compact_str::CompactString;
use satay_runtime::{
    Action, AllocStorage, BoxedStorage, BufferedResponse, OwnedAction, ResponseParts, Storage,
    from_json_slice_in,
};

use super::generated::*;
use super::policies::{Arena, Compact};

const RECORD: &str = r#"{"name":"Ada","labels":{"team":"ops"},"state":"future","children":[{"label":"kid"}],"choice":{"label":"chosen"}}"#;
const GRID: &str = r#"{"rows":[[1,2],[]],"points":[{"x":3}]}"#;

fn wire(json: &str) -> serde_json::Value {
    serde_json::from_str(json).unwrap()
}

fn response(body: &str) -> ResponseParts<Vec<u8>> {
    ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: body.as_bytes().to_vec(),
    }
}

#[test]
fn default_policy_keeps_std_collections() {
    let record: Record = serde_json::from_str(RECORD).unwrap();
    let name: &String = &record.name;
    let labels: &BTreeMap<String, String> = &record.labels;
    let children: &Vec<Child> = &record.children;
    assert_eq!((name.as_str(), labels["team"].as_str()), ("Ada", "ops"));
    assert_eq!(children[0].label, "kid");

    // Arrays without any text still follow the policy.
    let grid: Grid = serde_json::from_str(GRID).unwrap();
    let rows: &Vec<Vec<i32>> = &grid.rows;
    let points: &Vec<Point> = &grid.points;
    assert_eq!((rows.len(), points[0].x), (2, 3));
}

#[test]
fn owned_policies_round_trip_the_same_wire_form() {
    let standard: Record = serde_json::from_str(RECORD).unwrap();
    let boxed: generic::Record<'_, BoxedStorage> = serde_json::from_str(RECORD).unwrap();
    let compact: generic::Record<'_, Compact> = serde_json::from_str(RECORD).unwrap();

    let name: &Box<str> = &boxed.name;
    let children: &Box<[generic::Child<'_, BoxedStorage>]> = &boxed.children;
    assert_eq!((&**name, &*children[0].label), ("Ada", "kid"));
    let label: &CompactString = &compact.children[0].label;
    assert_eq!(label, "kid");
    assert_eq!(compact.labels.get("team").unwrap(), "ops");
    assert!(matches!(&boxed.state, generic::RecordState::Other(value) if &**value == "future"));
    assert_eq!(compact.state.to_string(), "future");

    assert_eq!(serde_json::to_value(&standard).unwrap(), wire(RECORD));
    assert_eq!(serde_json::to_value(&boxed).unwrap(), wire(RECORD));
    assert_eq!(serde_json::to_value(&compact).unwrap(), wire(RECORD));

    let grid: generic::Grid<'_, BoxedStorage> = serde_json::from_str(GRID).unwrap();
    let rows: &Box<[Box<[i32]>]> = &grid.rows;
    assert_eq!(&*rows[0], [1, 2]);
    assert_eq!(serde_json::to_value(&grid).unwrap(), wire(GRID));

    let _: generic::Label<'_, BoxedStorage> = "alias".into();
}

#[test]
fn models_format_compare_and_clone_without_policy_trait_impls() {
    let standard: Record = serde_json::from_str(RECORD).unwrap();
    let compact: generic::Record<'_, Compact> = serde_json::from_str(RECORD).unwrap();
    assert_eq!(format!("{compact:?}"), format!("{standard:?}"));
    assert_eq!(compact.clone(), compact);

    let mut changed = compact.clone();
    changed.children[0].label.push('!');
    assert_ne!(changed, compact);
}

#[test]
fn arena_policy_decodes_nested_models_into_the_arena() {
    let arena = Arena::default();
    let record: generic::Record<'_, Arena> = from_json_slice_in(RECORD.as_bytes(), &arena).unwrap();
    let children: &BumpVec<'_, generic::Child<'_, Arena>> = &record.children;
    let label: &BumpString<'_> = &children[0].label;
    assert_eq!(label, "kid");
    assert_eq!(record.labels.get("team").unwrap(), "ops");
    assert!(matches!(&record.state, generic::RecordState::Other(value) if value == "future"));
    assert!(
        matches!(&record.choice, Some(generic::RecordChoice::Child(child)) if child.label == "chosen")
    );
    assert_eq!(serde_json::to_value(&record).unwrap(), wire(RECORD));

    let grid: generic::Grid<'_, Arena> = from_json_slice_in(GRID.as_bytes(), &arena).unwrap();
    let rows: &BumpVec<'_, BumpVec<'_, i32>> = &grid.rows;
    let points: &BumpVec<'_, Point> = &grid.points;
    assert_eq!((&rows[0][..], points[0].x), (&[1, 2][..], 3));

    assert!(arena.0.allocated_bytes() > 0);

    let owned: Record = serde_json::from_str(RECORD).unwrap();
    assert_eq!(format!("{record:?}"), format!("{owned:?}"));
}

#[test]
fn buffered_responses_decode_into_the_policy_chosen_at_the_call() {
    let arena = Arena::default();

    let buffered = BufferedResponse::<ListLabelsAction<'_>, _>::new(response(r#"{"team":"ops"}"#));
    let ListLabelsResponse::Ok(labels) = buffered.decode().unwrap() else {
        panic!("expected labels")
    };
    let _: &BTreeMap<String, String> = &labels;
    let generic::ListLabelsResponse::Ok(labels) = buffered.decode_in(&BoxedStorage).unwrap() else {
        panic!("expected labels")
    };
    let (key, value): (&Box<str>, &Box<str>) = labels.first_key_value().unwrap();
    assert_eq!((&**key, &**value), ("team", "ops"));
    let generic::ListLabelsResponse::Ok(labels) = buffered.decode_in(&arena).unwrap() else {
        panic!("expected labels")
    };
    let (key, value): (&BumpString<'_>, &BumpString<'_>) = labels.first_key_value().unwrap();
    assert_eq!((key.as_str(), value.as_str()), ("team", "ops"));

    // Projected responses decode into the arena too, and do not keep the body.
    let buffered = BufferedResponse::<StoreRecordAction<'_, '_>, _>::new(response(&format!(
        "{{\"value\":{RECORD}}}"
    )));
    let generic::StoreRecordResponse::Ok(decoded) = buffered.decode_in(&arena).unwrap() else {
        panic!("expected record")
    };
    drop(buffered);
    assert_eq!(decoded.name, "Ada");
    assert_eq!(decoded.children[0].label, "kid");

    let buffered =
        BufferedResponse::<StoreGridsAction<'_, '_>, _>::new(response(&format!("[{GRID}]")));
    let generic::StoreGridsResponse::Ok(grids) = buffered.decode_in(&arena).unwrap() else {
        panic!("expected grids")
    };
    let _: &BumpVec<'_, generic::Grid<'_, Arena>> = &grids;
    assert_eq!(grids[0].points[0].x, 3);
}

#[test]
fn clients_select_an_owned_policy_for_inputs_and_context_free_decoding() {
    let api = Api::new().storage::<Compact>();
    let record: generic::Record<'_, Compact> = serde_json::from_str(RECORD).unwrap();
    let action = api.untagged().store_record("a/b", record);
    let request = action.clone().request().unwrap();
    assert_eq!(request.uri(), "/records/a%2Fb");
    assert_eq!(request.headers()["region"], "central");
    assert_eq!(request.headers()["validated"], "ok");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(request.body()).unwrap(),
        wire(RECORD)
    );
    assert!(format!("{action:?}").contains("StoreRecordInput"));

    let _ = <StoreRecordAction<'_, '_, Compact> as Action>::decode;
    let parts = response(&format!("{{\"value\":{RECORD}}}"));
    let owned: generic::StoreRecordResponse<'static, Compact> =
        StoreRecordAction::<Compact>::decode_owned(parts.as_bytes()).unwrap();
    drop(parts);
    let generic::StoreRecordResponse::Ok(owned) = owned else {
        panic!("expected owned record")
    };
    assert_eq!(owned.name, "Ada");

    // Array parameters and array bodies keep their wire encoding.
    let grid: Grid = serde_json::from_str(GRID).unwrap();
    let request = Api::new()
        .untagged()
        .store_grids(vec!["a b".to_owned(), "c".to_owned()], vec![grid])
        .request()
        .unwrap();
    assert_eq!(request.uri(), "/grids?tag=a%20b&tag=c");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(request.body()).unwrap(),
        wire(&format!("[{GRID}]"))
    );
}

#[test]
fn arena_inputs_encode_without_a_storage_context() {
    let arena = Arena::default();
    let record: generic::Record<'_, Arena> = from_json_slice_in(RECORD.as_bytes(), &arena).unwrap();
    let request = operations::store_record::encode_store_record(generic::StoreRecordInput {
        key: arena.try_text("a/b").unwrap(),
        region: None,
        validated: None,
        body: record,
    })
    .unwrap();
    assert_eq!(request.uri(), "/records/a%2Fb");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(request.body()).unwrap(),
        wire(RECORD)
    );

    let grid: generic::Grid<'_, Arena> = from_json_slice_in(GRID.as_bytes(), &arena).unwrap();
    // Projections alone do not identify a policy, so the literal names it.
    let request = operations::store_grids::encode_store_grids(generic::StoreGridsInput::<Arena> {
        tag: arena
            .try_contiguous([arena.try_text("x").unwrap()])
            .unwrap(),
        body: arena.try_contiguous([grid]).unwrap(),
    })
    .unwrap();
    assert_eq!(request.uri(), "/grids?tag=x");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(request.body()).unwrap(),
        wire(&format!("[{GRID}]"))
    );
}

#[test]
fn default_policy_code_names_no_policy() {
    use super::default_policy::{Cache, describe, input, record, stored};

    let built = record("Ada".to_owned());
    let wire = serde_json::to_string(&built).unwrap();
    let decoded: Record = serde_json::from_str(&wire).unwrap();
    assert_eq!(decoded, built);
    assert!(describe(&decoded).starts_with("Record { name: \"Ada\""));

    // The same types flow through the client without annotations.
    let request = Api::new()
        .untagged()
        .store_record("a/b", decoded.clone())
        .request()
        .unwrap();
    assert_eq!(request.headers()["region"], "central");
    let request = operations::store_record::encode_store_record(input(decoded)).unwrap();
    assert_eq!(request.headers()["region"], "west");

    let parts = response(&format!("{{\"value\":{wire}}}"));
    let saved = stored(StoreRecordAction::decode(parts.as_bytes()).unwrap());
    let mut cache = Cache {
        last: saved,
        grids: vec![],
    };
    cache.grids.push(serde_json::from_str(GRID).unwrap());
    assert_eq!(cache.last, Some(built));
}

#[test]
fn owned_and_context_decoders_agree() {
    // Owned policies decode through Serde's derive and a storage context through
    // a generated decoder. The same object must mean the same thing to both.
    // A valid record with one field replaced or added.
    let record = |field: &str, value: &str| {
        let mut fields = vec![
            ("name", r#""Ada""#),
            ("labels", "{}"),
            ("state", r#""ready""#),
            ("children", "[]"),
        ];
        match fields.iter_mut().find(|(name, _)| *name == field) {
            Some(slot) => slot.1 = value,
            None => fields.push((field, value)),
        }
        let fields = fields
            .iter()
            .map(|(name, value)| format!("{name:?}:{value}"))
            .collect::<Vec<_>>();
        format!("{{{}}}", fields.join(","))
    };
    let accepted = [
        RECORD.to_owned(),
        record("unknown", r#"{"deep":[1,null]}"#),
        record("state", r#""future""#),
        record("labels", r#"{"a":"x","a":"y"}"#),
        record("choice", "null"),
        record("choice", "7"),
        record("choice", r#"{"label":"chosen","extra":true}"#),
        record("pet", r#"{"kind":"dog","name":"Rex"}"#),
        record("pet", r#"{"name":"Tom","lives":9,"kind":"cat"}"#),
    ];
    let rejected = [
        r#"{"name":"Ada","name":"Bo","labels":{},"state":"ready","children":[]}"#.to_owned(),
        r#"{"labels":{},"state":"ready","children":[]}"#.to_owned(),
        record("state", "1"),
        record("labels", r#"{"a":1}"#),
        record("children", r#"[{"label":"kid"},{}]"#),
        record("children", "null"),
        record("choice", "7.5"),
        record("choice", r#"{"label":1}"#),
        record("pet", r#"{"kind":"cat","name":"Tom","lives":"nine"}"#),
        record("pet", r#"{"kind":"bird","name":"Tweety"}"#),
        record("pet", r#"{"kind":"dog","kind":"cat","name":"Rex"}"#),
        record("pet", r#"{"kind":1,"name":"Rex"}"#),
        record("pet", r#"{"name":"Rex"}"#),
        record("pet", "[]"),
        format!("{RECORD} trailing"),
        "null".to_owned(),
    ];
    let decode = |input: &str| {
        let owned = serde_json::from_str::<Record>(input).ok();
        let context = from_json_slice_in::<generic::Record<'_, AllocStorage>, _>(
            input.as_bytes(),
            &AllocStorage,
        )
        .ok();
        assert_eq!(owned, context, "{input}");
        owned
    };
    for input in &accepted {
        assert!(decode(input).is_some(), "{input}");
    }
    for input in &rejected {
        assert!(decode(input).is_none(), "{input}");
    }
}

#[test]
fn owned_models_decode_from_formats_without_field_names() {
    // Binary Serde formats write a struct as its fields in order.
    let child: Child = serde_json::from_str(r#"["kid"]"#).unwrap();
    assert_eq!(child.label, "kid");
    let boxed: generic::Grid<'_, BoxedStorage> =
        serde_json::from_str("[[[1,2]],[{\"x\":3}]]").unwrap();
    assert_eq!((&*boxed.rows[0], boxed.points[0].x), (&[1, 2][..], 3));
}
