use super::generated::*;
use std::{collections::BTreeMap, convert::Infallible};
use bumpalo::{Bump, collections::{String as BumpString, Vec as BumpVec}};
use satay_runtime::{storage::{Storage, AllocStorage, BoxedStorage}, storage_serde::{CollectionStorage, DecodeContext, DecodeError}};
use serde::de::{DeserializeSeed, SeqAccess};

// Neither the policy nor its collection implements Clone, Debug, or Serde.
struct Arena(Bump);
struct ReadOnly<'a,T>(BumpVec<'a,T>);
impl<T> AsRef<[T]> for ReadOnly<'_,T> { fn as_ref(&self) -> &[T] { self.0.as_slice() } }
impl Storage for Arena {
    type Error = Infallible;
    type Text<'a> = BumpString<'a>;
    type Contiguous<'a,T:'a> = ReadOnly<'a,T>;
    type Map<'a,V:'a> = BTreeMap<BumpString<'a>,V>;
    fn try_text<'a>(&'a self, value: &str) -> Result<Self::Text<'a>,Self::Error> { Ok(BumpString::from_str_in(value, &self.0)) }
    fn try_contiguous<'a,T:'a>(&'a self, values: impl IntoIterator<Item=T>) -> Result<Self::Contiguous<'a,T>,Self::Error> { Ok(ReadOnly(BumpVec::from_iter_in(values, &self.0))) }
    fn try_map<'a,V:'a>(&'a self, values: impl IntoIterator<Item=(Self::Text<'a>,V)>) -> Result<Self::Map<'a,V>,Self::Error> { Ok(values.into_iter().collect()) }
}
impl CollectionStorage for Arena {
    fn deserialize_contiguous<'storage,'de,T:'storage,A,E>(context: &DecodeContext<'storage,Self>, mut sequence:A, element:E) -> Result<Self::Contiguous<'storage,T>,A::Error>
    where A:SeqAccess<'de>, E:DeserializeSeed<'de,Value=T>+Clone {
        let mut values = BumpVec::new_in(&context.storage().0);
        while let Some(value) = sequence.next_element_seed(element.clone())? { values.push(value); }
        Ok(ReadOnly(values))
    }
}
const BODY: &str = r#"{"pets":[{"name":"Milo","counts":[2,1]}],"matrix":[[3,4],[]],"flags":[true,false],"choice":{"name":"Otis","counts":[]},"lossy":false,"mapped":{"first":{"name":"Milo","counts":[2,1]}}}"#;

#[test]
fn arena_decodes_nested_models_and_encodes_requests_without_container_serde() {
    let arena = Arena(Bump::new());
    let model = { let input = BODY.as_bytes().to_vec(); Envelope::<Arena>::from_json_in(&arena, &input).unwrap() };
    assert_eq!(model.pets.as_ref()[0].name.as_str(), "Milo");
    assert_eq!(model.matrix.as_ref()[0].as_ref(), [3,4]);
    assert!(model.lossy.is_none());
    assert!(matches!(model.choice, Some(EnvelopeChoice::Pet(_))));
    let wire = serde_json::to_value(&model).unwrap();
    assert_eq!(wire["pets"][0]["counts"], serde_json::json!([2,1]));
    let api = Api::new().storage_in(&arena);
    let request = api.untagged().save_pets(arena.try_contiguous([7,8]).unwrap(), model).request().unwrap();
    assert_eq!(request.uri(), "/pets?counts=7&counts=8");
    assert_eq!(request.headers()["region"], "central");
    let response_body = format!("{{\"result\":{BODY}}}");
    let response = satay_runtime::ResponseParts { status: http::StatusCode::OK, headers:http::HeaderMap::new(), body:response_body.as_bytes() };
    let decoded = operations::save_pets::decode_save_pets_response_in(&arena, response).unwrap();
    drop(response_body);
    let SavePetsResponse::Ok(decoded) = decoded else { panic!("unexpected status") };
    assert_eq!(decoded.pets.as_ref()[0].name.as_str(), "Milo");
}

#[test]
fn owned_and_boxed_models_and_numeric_aliases() {
    let model: owned::Envelope = serde_json::from_str(BODY).unwrap();
    let boxed: Envelope<'static,BoxedStorage> = serde_json::from_str(BODY).unwrap();
    assert_eq!(serde_json::to_value(model).unwrap(), serde_json::to_value(boxed).unwrap());
    let _: Counts<'static> = vec![1,2];
    let _: Counts<'static,BoxedStorage> = vec![1,2].into_boxed_slice();
    let node: owned::Node = serde_json::from_str(r#"{"name":"root","children":[{"name":"leaf","children":[]}]}"#).unwrap();
    assert_eq!(node.children[0].name, "leaf");
    assert_eq!(node, node.clone());
    assert_eq!(serde_json::to_value(&node).unwrap()["name"], "root");
}

#[derive(Debug, PartialEq)]
enum Exhausted { Text, Collection }
struct Failing { text: bool, collection: bool }
impl Storage for Failing {
    type Error = Exhausted;
    type Text<'a> = String;
    type Contiguous<'a,T:'a> = Vec<T>;
    type Map<'a,V:'a> = BTreeMap<String,V>;
    fn try_text<'a>(&'a self, value:&str)->Result<String,Exhausted> { if self.text { Err(Exhausted::Text) } else { Ok(value.into()) } }
    fn try_contiguous<'a,T:'a>(&'a self, values:impl IntoIterator<Item=T>)->Result<Vec<T>,Exhausted> { if self.collection { Err(Exhausted::Collection) } else { Ok(values.into_iter().collect()) } }
    fn try_map<'a,V:'a>(&'a self, values:impl IntoIterator<Item=(String,V)>)->Result<BTreeMap<String,V>,Exhausted> { Ok(values.into_iter().collect()) }
}
impl CollectionStorage for Failing {
    fn deserialize_contiguous<'storage,'de,T:'storage,A,E>(context:&DecodeContext<'storage,Self>, sequence:A, element:E)->Result<Vec<T>,A::Error>
    where A:SeqAccess<'de>, E:DeserializeSeed<'de,Value=T>+Clone {
        let values = AllocStorage::deserialize_contiguous(&DecodeContext::new(&AllocStorage),sequence,element)?;
        context.storage().try_contiguous(values).map_err(|error|context.storage_error(error))
    }
}
#[test]
fn typed_failures_survive_lossy_fields_unions_collections_and_defaults() {
    let text = Failing { text:true, collection:false };
    let collection = Failing { text:false, collection:true };
    assert!(matches!(Envelope::<Failing>::from_json_in(&text,BODY.as_bytes()),Err(DecodeError::Storage(Exhausted::Text))));
    assert!(matches!(Envelope::<Failing>::from_json_in(&collection,BODY.as_bytes()),Err(DecodeError::Storage(Exhausted::Collection))));
    for field in ["lossy","choice"] {
        let input = format!(r#"{{"pets":[],"matrix":[],"flags":[],"{field}":{{"name":"fail","counts":[]}}}}"#);
        assert!(matches!(Envelope::<Failing>::from_json_in(&text,input.as_bytes()),Err(DecodeError::Storage(Exhausted::Text))));
    }
    let model = Envelope::<Failing> { pets:vec![],matrix:vec![],flags:vec![],choice:None,lossy:None,mapped:None };
    let api = Api::new().storage_in(&text);
    assert!(matches!(api.untagged().try_save_pets(vec![],model),Err(Exhausted::Text)));
    let arena = Arena(Bump::new());
    for bytes in [b"{".as_slice(), br#"{"name":"Milo","counts":[1,false]}"#] {
        assert!(matches!(Pet::<Arena>::from_json_in(&arena,bytes),Err(DecodeError::Decode(_))));
    }
}

#[test]
fn arena_value_traits_do_not_require_traits_on_policy_or_containers() {
    let arena = Arena(Bump::new());
    let left = Envelope::<Arena>::from_json_in(&arena,BODY.as_bytes()).unwrap();
    let right = Envelope::<Arena>::from_json_in(&arena,BODY.as_bytes()).unwrap();
    assert_eq!(left,right);
    assert!(format!("{left:?}").contains("Milo"));
}

// These functions compile against the transports' actual raw HTTP entry points.
// Build the request before creating a Send future: the arena is not Sync.
fn reqwest_raw(request: http::Request<Vec<u8>>) -> impl Future<Output = reqwest::Response> + Send {
    async move {
        let request = request.map(reqwest::Body::from).try_into().unwrap();
        reqwest::Client::new().execute(request).await.unwrap()
    }
}
async fn reqwest_arena_flow(arena: &Arena, request: http::Request<Vec<u8>>) -> SavePetsResponse<'_, Arena> {
    let response = reqwest_raw(request).await;
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.bytes().await.unwrap();
    operations::save_pets::decode_save_pets_response_in(arena, satay_runtime::ResponseParts { status, headers, body: &bytes }).unwrap()
}
fn ureq_arena_flow(arena: &Arena, request: http::Request<Vec<u8>>) -> SavePetsResponse<'_, Arena> {
    use std::io::Read;
    let (parts, body) = request.into_parts();
    let response = ureq::Agent::new_with_defaults().run(http::Request::from_parts(parts, body.as_slice())).unwrap();
    let (parts, body) = response.into_parts();
    let mut bytes = Vec::new();
    body.into_reader().read_to_end(&mut bytes).unwrap();
    operations::save_pets::decode_save_pets_response_in(arena, satay_runtime::ResponseParts { status: parts.status, headers: parts.headers, body: &bytes }).unwrap()
}

#[test]
fn arena_tagged_unions_and_scalar_arrays_preserve_validation() {
    let arena = Arena(Bump::new());
    let tagged = Tagged::<Arena>::from_json_in(&arena, br#"{"kind":"pet","name":"Milo","counts":[1]}"#).unwrap();
    assert!(matches!(tagged, Tagged::Pet(_)));
    assert_eq!(serde_json::to_value(tagged).unwrap()["kind"], "pet");
    let scalars = Scalars::<Arena>::from_json_in(&arena, br#"{"states":["ready","pending"],"bounded":[0,9]}"#).unwrap();
    assert_eq!(scalars.states.as_ref(), [State::Ready, State::Pending]);
    assert_eq!(serde_json::to_value(scalars).unwrap()["bounded"], serde_json::json!([0,9]));
    assert!(Scalars::<Arena>::from_json_in(&arena, br#"{"states":[],"bounded":[10]}"#).is_err());
    assert!(Scalars::<Arena>::from_json_in(&arena, br#"{"states":["invalid"],"bounded":[]}"#).is_err());
}
