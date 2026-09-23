use super::*;
use crate::model::{
    ApiGroup, Component, ComponentKind, GroupOperation, HttpMethod, RequestBody, ResponseCase,
};
use crate::model::{PathSegment, ResponseStatus};
use quote::{ToTokens, quote};
use syn::{Fields, GenericArgument, Item, PathArguments, Type};

#[test]
fn render_file_exposes_struct_ast_without_source_comparison() {
    let api = Api::new(
        String::new(),
        vec![],
        vec![Component {
            rust_name: "Pet".to_owned(),
            description: None,
            kind: ComponentKind::Struct(vec![
                Field {
                    wire_name: "id".to_owned(),
                    identifier_words: None,
                    rust_name: "id".to_owned(),
                    description: None,
                    ty: TypeRef::String,
                    required: true,
                    treat_error_as_none: false,
                    none_if: vec![],
                },
                Field {
                    wire_name: "tag_count".to_owned(),
                    identifier_words: None,
                    rust_name: "tag_count".to_owned(),
                    description: None,
                    ty: TypeRef::Integer(IntegerType::I32),
                    required: false,
                    treat_error_as_none: false,
                    none_if: vec![],
                },
            ]),
        }],
        vec![],
        vec![],
        vec![],
    );

    let file = types::render_types_file(&api);
    assert_eq!(file.items.len(), 1);
    let Item::Struct(item) = &file.items[0] else {
        panic!("expected struct item");
    };
    assert_eq!(item.ident, "Pet");
    let Fields::Named(fields) = &item.fields else {
        panic!("expected named fields");
    };
    assert_eq!(fields.named.len(), 2);

    let mut fields = fields.named.iter();
    let id = fields.next().expect("id field");
    assert_eq!(id.ident.as_ref().expect("field ident"), "id");
    assert!(type_path_is(&id.ty, "__SatayText"));

    let tag_count = fields.next().expect("tag_count field");
    assert_eq!(tag_count.ident.as_ref().expect("field ident"), "tag_count");
    let Some(inner) = option_inner(&tag_count.ty) else {
        panic!("optional field should render as Option<T>");
    };
    assert!(type_path_is(inner, "i32"));
}

#[test]
fn render_file_exposes_operation_items_without_source_comparison() {
    let api = Api::new(
        String::new(),
        vec![],
        vec![],
        vec![],
        vec![ApiGroup {
            wire_name: Some("pets".to_owned()),
            rust_name: "pets".to_owned(),
            description: None,
            operations: vec![GroupOperation {
                operation_index: 0,
                method_name: "create_pet".to_owned(),
            }],
        }],
        vec![Operation {
            fn_name: "create_pet".to_owned(),
            tags: vec!["pets".to_owned()],
            description: None,
            input_name: "CreatePetInput".to_owned(),
            response_name: "CreatePetResponse".to_owned(),
            method: HttpMethod::Post,
            path: "/pets".to_owned(),
            path_segments: vec![PathSegment::Literal("/pets".to_owned())],
            parameters: vec![],
            request_body: Some(RequestBody {
                field_name: "body".to_owned(),
                description: None,
                content_type: "application/json".to_owned(),
                ty: TypeRef::Named("Pet".to_owned()),
                required: true,
            }),
            responses: vec![ResponseCase {
                status: ResponseStatus::Exact(201),
                variant_name: "Created".to_owned(),
                description: None,
                body: Some(TypeRef::Named("Pet".to_owned())),
                projection: None,
            }],
        }],
    );

    let files = render_api(&api, GenerateOptions::default());
    assert!(files.iter().any(|f| f.relative_path == "mod.rs"));
    assert!(files.iter().any(|f| f.relative_path == "create_pet/mod.rs"));
    assert!(files.iter().any(|f| f.relative_path == "pets.rs"));
    assert!(
        files
            .iter()
            .any(|f| f.relative_path == "create_pet/parts.rs")
    );
    assert!(
        files
            .iter()
            .any(|f| f.relative_path == "create_pet/json.rs")
    );
}

#[test]
fn rust_field_type_wraps_optional_and_treat_error_as_none_fields() {
    assert_eq!(
        rust_field_type(&TypeRef::String, true, false)
            .to_token_stream()
            .to_string(),
        "__SatayText"
    );
    assert_eq!(
        rust_field_type(&TypeRef::String, false, false)
            .to_token_stream()
            .to_string(),
        "Option < __SatayText >"
    );
    assert_eq!(
        rust_field_type(&TypeRef::String, true, true)
            .to_token_stream()
            .to_string(),
        "Option < __SatayText >"
    );
    assert_eq!(
        rust_field_type(&TypeRef::Option(Box::new(TypeRef::String)), true, false)
            .to_token_stream()
            .to_string(),
        "Option < __SatayText >"
    );
}

#[test]
fn input_builder_arguments_convert_strings_only() {
    assert_eq!(
        input_builder_arg_type(&TypeRef::String).to_string(),
        "impl Into < __SatayText >"
    );
    assert_eq!(
        input_builder_arg_type(&TypeRef::Integer(IntegerType::I32)).to_string(),
        "i32"
    );
    assert_eq!(
        input_builder_value(quote!(value), &TypeRef::String).to_string(),
        "value . into ()"
    );
    assert_eq!(
        input_builder_value(quote!(value), &TypeRef::Integer(IntegerType::I32)).to_string(),
        "value"
    );
}

#[test]
fn request_conversion_mode_matches_body_requirement() {
    assert_eq!(
        request_from_parts_expr(&operation_with_body(None))
            .to_token_stream()
            .to_string(),
        "satay_runtime :: into_empty_request (parts)"
    );
    assert_eq!(
        request_from_parts_expr(&operation_with_body(Some(true)))
            .to_token_stream()
            .to_string(),
        "satay_runtime :: into_json_request (parts)"
    );
    assert_eq!(
        request_from_parts_expr(&operation_with_body(Some(false)))
            .to_token_stream()
            .to_string(),
        "satay_runtime :: into_optional_json_request (parts)"
    );
}

fn operation_with_body(required: Option<bool>) -> Operation {
    Operation {
        fn_name: "create_pet".to_owned(),
        tags: vec![],
        description: None,
        input_name: "CreatePetInput".to_owned(),
        response_name: "CreatePetResponse".to_owned(),
        method: HttpMethod::Post,
        path: "/pets".to_owned(),
        path_segments: vec![PathSegment::Literal("/pets".to_owned())],
        parameters: vec![],
        request_body: required.map(|required| RequestBody {
            field_name: "body".to_owned(),
            description: None,
            content_type: "application/json".to_owned(),
            ty: TypeRef::Named("Pet".to_owned()),
            required,
        }),
        responses: vec![],
    }
}

fn type_path_is(ty: &syn::Type, expected: &str) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    path.path.is_ident(expected)
}

fn option_inner(ty: &syn::Type) -> Option<&syn::Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.first()?;
    if segment.ident != "Option" {
        return None;
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let GenericArgument::Type(inner) = arguments.args.first()? else {
        return None;
    };
    Some(inner)
}
