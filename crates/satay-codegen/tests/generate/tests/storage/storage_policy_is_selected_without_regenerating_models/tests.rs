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
    let boxed: Record<'_, BoxedStorage> = serde_json::from_str(RECORD).unwrap();
    let compact: Record<'_, Compact> = serde_json::from_str(RECORD).unwrap();

    let name: &Box<str> = &boxed.name;
    let children: &Box<[Child<'_, BoxedStorage>]> = &boxed.children;
    assert_eq!((&**name, &*children[0].label), ("Ada", "kid"));
    let label: &CompactString = &compact.children[0].label;
    assert_eq!(label, "kid");
    assert_eq!(compact.labels.get("team").unwrap(), "ops");
    assert!(matches!(&boxed.state, RecordState::Other(value) if &**value == "future"));
    assert_eq!(compact.state.to_string(), "future");

    assert_eq!(serde_json::to_value(&standard).unwrap(), wire(RECORD));
    assert_eq!(serde_json::to_value(&boxed).unwrap(), wire(RECORD));
    assert_eq!(serde_json::to_value(&compact).unwrap(), wire(RECORD));

    let grid: Grid<'_, BoxedStorage> = serde_json::from_str(GRID).unwrap();
    let rows: &Box<[Box<[i32]>]> = &grid.rows;
    assert_eq!(&*rows[0], [1, 2]);
    assert_eq!(serde_json::to_value(&grid).unwrap(), wire(GRID));

    let _: Label<'_, BoxedStorage> = "alias".into();
}

#[test]
fn models_format_compare_and_clone_without_policy_trait_impls() {
    let standard: Record = serde_json::from_str(RECORD).unwrap();
    let compact: Record<'_, Compact> = serde_json::from_str(RECORD).unwrap();
    assert_eq!(format!("{compact:?}"), format!("{standard:?}"));
    assert_eq!(compact.clone(), compact);

    let mut changed = compact.clone();
    changed.children[0].label.push('!');
    assert_ne!(changed, compact);
}

#[test]
fn arena_policy_decodes_nested_models_into_the_arena() {
    let arena = Arena::default();
    let record: Record<'_, Arena> = from_json_slice_in(RECORD.as_bytes(), &arena).unwrap();
    let children: &BumpVec<'_, Child<'_, Arena>> = &record.children;
    let label: &BumpString<'_> = &children[0].label;
    assert_eq!(label, "kid");
    assert_eq!(record.labels.get("team").unwrap(), "ops");
    assert!(matches!(&record.state, RecordState::Other(value) if value == "future"));
    assert!(
        matches!(&record.choice, Some(RecordChoice::Child(child)) if child.label == "chosen")
    );
    assert_eq!(serde_json::to_value(&record).unwrap(), wire(RECORD));

    let grid: Grid<'_, Arena> = from_json_slice_in(GRID.as_bytes(), &arena).unwrap();
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
    let ListLabelsResponse::Ok(labels) = buffered.decode_in(&BoxedStorage).unwrap() else {
        panic!("expected labels")
    };
    let (key, value): (&Box<str>, &Box<str>) = labels.first_key_value().unwrap();
    assert_eq!((&**key, &**value), ("team", "ops"));
    let ListLabelsResponse::Ok(labels) = buffered.decode_in(&arena).unwrap() else {
        panic!("expected labels")
    };
    let (key, value): (&BumpString<'_>, &BumpString<'_>) = labels.first_key_value().unwrap();
    assert_eq!((key.as_str(), value.as_str()), ("team", "ops"));

    // Projected responses decode into the arena too, and do not keep the body.
    let buffered = BufferedResponse::<StoreRecordAction<'_, '_>, _>::new(response(&format!(
        "{{\"value\":{RECORD}}}"
    )));
    let StoreRecordResponse::Ok(decoded) = buffered.decode_in(&arena).unwrap() else {
        panic!("expected record")
    };
    drop(buffered);
    assert_eq!(decoded.name, "Ada");
    assert_eq!(decoded.children[0].label, "kid");

    let buffered =
        BufferedResponse::<StoreGridsAction<'_, '_>, _>::new(response(&format!("[{GRID}]")));
    let StoreGridsResponse::Ok(grids) = buffered.decode_in(&arena).unwrap() else {
        panic!("expected grids")
    };
    let _: &BumpVec<'_, Grid<'_, Arena>> = &grids;
    assert_eq!(grids[0].points[0].x, 3);
}

#[test]
fn clients_select_an_owned_policy_for_inputs_and_context_free_decoding() {
    let api = Api::new().storage::<Compact>();
    let record: Record<'_, Compact> = serde_json::from_str(RECORD).unwrap();
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
    let owned: StoreRecordResponse<'static, Compact> =
        StoreRecordAction::<Compact>::decode_owned(parts.as_bytes()).unwrap();
    drop(parts);
    let StoreRecordResponse::Ok(owned) = owned else {
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
    let record: Record<'_, Arena> = from_json_slice_in(RECORD.as_bytes(), &arena).unwrap();
    let request = operations::store_record::encode_store_record(StoreRecordInput {
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

    let grid: Grid<'_, Arena> = from_json_slice_in(GRID.as_bytes(), &arena).unwrap();
    // Projections alone do not identify a policy, so the literal names it.
    let request = operations::store_grids::encode_store_grids(StoreGridsInput::<Arena> {
        tag: arena.try_contiguous([arena.try_text("x").unwrap()]).unwrap(),
        body: arena.try_contiguous([grid]).unwrap(),
    })
    .unwrap();
    assert_eq!(request.uri(), "/grids?tag=x");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(request.body()).unwrap(),
        wire(&format!("[{GRID}]"))
    );
    let _ = AllocStorage;
}
