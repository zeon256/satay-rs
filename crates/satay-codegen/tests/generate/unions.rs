use std::fs;

use crate::ast::*;
use crate::common::*;

#[test]
fn any_of_generates_untagged_union_types() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Search API
  version: 1.0.0
paths:
  /search:
    get:
      operationId: search
      responses:
        '200':
          description: Search result
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/SearchResult'
components:
  schemas:
    User:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    Organization:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    SearchResult:
      description: A search result.
      anyOf:
        - $ref: '#/components/schemas/User'
        - $ref: '#/components/schemas/Organization'
"##,
    )
    .expect("generate anyOf fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let union = find_enum(&types_rs, "SearchResult");
    assert_doc(&union.attrs, "A search result.");
    assert_attr_contains(&union.attrs, "cfg_attr", "serde(untagged)");

    assert_eq!(variant_names(union), ["User", "Organization"]);
    assert_eq!(norm(&variant(union, "User").fields), norm_str("(User)"));
    assert_eq!(
        norm(&variant(union, "Organization").fields),
        norm_str("(Organization)")
    );
}

#[test]
fn any_of_discriminator_generates_tagged_union_types() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Search API
  version: 1.0.0
paths:
  /search:
    get:
      operationId: search
      responses:
        '200':
          description: Search result
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/SearchResult'
components:
  schemas:
    User:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    Organization:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    SearchResult:
      anyOf:
        - $ref: '#/components/schemas/User'
        - $ref: '#/components/schemas/Organization'
      discriminator:
        propertyName: kind
"##,
    )
    .expect("generate discriminator anyOf fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let union = find_enum(&types_rs, "SearchResult");
    assert_attr_contains(&union.attrs, "cfg_attr", r#"serde(tag = "kind")"#);
    assert!(!contains_tokens(&types_rs, "serde(untagged)"));
    assert_attr_contains(
        &variant(union, "User").attrs,
        "cfg_attr",
        r#"serde(rename = "User")"#,
    );
    assert_attr_contains(
        &variant(union, "Organization").attrs,
        "cfg_attr",
        r#"serde(rename = "Organization")"#,
    );
}

#[test]
fn one_of_discriminator_mapping_generates_variant_renames() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Pet API
  version: 1.0.0
paths:
  /pet:
    get:
      operationId: getPet
      responses:
        '200':
          description: Pet
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Pet'
components:
  schemas:
    Dog:
      type: object
      required:
        - name
      properties:
        name:
          type: string
    Cat:
      type: object
      required:
        - name
      properties:
        name:
          type: string
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
        mapping:
          dog: '#/components/schemas/Dog'
"##,
    )
    .expect("generate discriminator oneOf fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let union = find_enum(&types_rs, "Pet");
    assert_attr_contains(&union.attrs, "cfg_attr", r#"serde(tag = "kind")"#);
    assert_attr_contains(
        &variant(union, "Dog").attrs,
        "cfg_attr",
        r#"serde(rename = "dog")"#,
    );
    assert_attr_contains(
        &variant(union, "Cat").attrs,
        "cfg_attr",
        r#"serde(rename = "Cat")"#,
    );
}

#[test]
fn generated_any_of_deserializes_with_first_matching_branch() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Entity API
  version: 1.0.0
paths:
  /entity:
    get:
      operationId: getEntity
      responses:
        '200':
          description: Entity
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Entity'
components:
  schemas:
    Loose:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    Specific:
      type: object
      required:
        - id
        - slug
      properties:
        id:
          type: string
        slug:
          type: string
    Entity:
      anyOf:
        - $ref: '#/components/schemas/Loose'
        - $ref: '#/components/schemas/Specific'
"##,
    )
    .expect("generate anyOf runtime fixture");

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn any_of_uses_first_matching_branch() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"id":"1","slug":"specific"}"#.to_vec(),
        };

        let decoded = decode_get_entity_response(response).expect("decoded response");
        match decoded {
            GetEntityResponse::Ok(Entity::Loose(value)) => {
                assert_eq!(value.id, "1");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(crate_dir, "test", &[], "anyOf generated crate tests");
}

#[test]
fn generated_discriminator_union_serializes_and_deserializes_with_tag() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Pet API
  version: 1.0.0
paths:
  /pet:
    get:
      operationId: getPet
      responses:
        '200':
          description: Pet
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Pet'
components:
  schemas:
    Dog:
      type: object
      required:
        - name
        - barkVolume
      properties:
        name:
          type: string
        barkVolume:
          type: integer
    Cat:
      type: object
      required:
        - name
        - lives
      properties:
        name:
          type: string
        lives:
          type: integer
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
        mapping:
          dog: '#/components/schemas/Dog'
          cat: Cat
"##,
    )
    .expect("generate discriminator runtime fixture");

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn tagged_union_deserializes_response() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"kind":"cat","name":"Milo","lives":9}"#.to_vec(),
        };

        let decoded = decode_get_pet_response(response).expect("decoded response");
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
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "discriminator generated crate tests",
    );
}
