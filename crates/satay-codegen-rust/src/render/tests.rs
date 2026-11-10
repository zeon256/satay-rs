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

    let file = types::render_types_file(&api, &Family::new(&api));
    let item = file
        .items
        .iter()
        .find_map(|item| match item {
            Item::Struct(item) => Some(item),
            _ => None,
        })
        .expect("struct item");
    assert_eq!(item.ident, "Pet");
    let Fields::Named(fields) = &item.fields else {
        panic!("expected named fields");
    };
    assert_eq!(fields.named.len(), 2);

    let mut fields = fields.named.iter();
    let id = fields.next().expect("id field");
    assert_eq!(id.ident.as_ref().expect("field ident"), "id");
    assert_eq!(id.ty.to_token_stream().to_string(), "S :: Text < 's >");
    assert_eq!(item.generics.lifetimes().count(), 1);
    assert_eq!(item.generics.type_params().count(), 1);

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

fn component(name: &str, kind: ComponentKind) -> Component {
    Component {
        rust_name: name.to_owned(),
        description: None,
        kind,
    }
}

fn field(name: &str, ty: TypeRef) -> Field {
    Field {
        wire_name: name.to_owned(),
        identifier_words: None,
        rust_name: name.to_owned(),
        description: None,
        ty,
        required: true,
        treat_error_as_none: false,
        none_if: vec![],
    }
}

fn named(name: &str) -> TypeRef {
    TypeRef::Named(name.to_owned())
}

fn family_api() -> Api {
    Api::new(
        String::new(),
        vec![],
        vec![
            component(
                "Tags",
                ComponentKind::Alias(TypeRef::Array(Box::new(TypeRef::String))),
            ),
            component(
                "Count",
                ComponentKind::Struct(vec![field("value", TypeRef::Integer(IntegerType::I32))]),
            ),
            component(
                "Pet",
                ComponentKind::Struct(vec![field("tags", named("Tags"))]),
            ),
            component(
                "Owner",
                ComponentKind::Struct(vec![field("pet", named("Pet"))]),
            ),
        ],
        vec![],
        vec![],
        vec![],
    )
}

#[test]
fn family_membership_propagates_through_aliases_and_models() {
    let family = Family::new(&family_api());
    assert!(family.is_generic("Tags"));
    assert!(family.is_generic("Pet"));
    assert!(family.is_generic("Owner"));
    assert!(!family.is_generic("Count"));
}

#[test]
fn family_types_follow_the_requested_spelling() {
    let family = Family::new(&family_api());
    let ty = TypeRef::Map(Box::new(TypeRef::Array(Box::new(named("Pet")))));
    let spelled = |spelling| family.ty(&ty, spelling).to_token_stream().to_string();
    assert_eq!(
        spelled(Spelling::Bounded),
        "S :: Map < 's , S :: Contiguous < 's , Pet < 's , S > > >"
    );
    assert_eq!(
        spelled(Spelling::Qualified),
        "< S as satay_runtime :: Storage > :: Map < 's , < S as satay_runtime :: Storage > :: \
         Contiguous < 's , Pet < 's , S > > >"
    );
    assert_eq!(
        spelled(Spelling::Concrete),
        "BTreeMap < String , Vec < Pet < 'static > > >"
    );
    assert_eq!(
        family
            .field_ty(&named("Count"), true, Spelling::Bounded)
            .to_token_stream()
            .to_string(),
        "Option < Count >"
    );
    assert_eq!(
        family
            .field_ty(&TypeRef::option(TypeRef::String), true, Spelling::Bounded)
            .to_token_stream()
            .to_string(),
        "Option < S :: Text < 's > >"
    );
}

#[test]
fn shapes_resolve_aliases_and_separate_models_from_projections() {
    let family = Family::new(&family_api());
    let shape = |ty: &TypeRef| family.shape(ty).to_string();
    assert_eq!(
        shape(&named("Tags")),
        "shape :: Contiguous < shape :: Text >"
    );
    assert_eq!(shape(&named("Pet")), "shape :: Model < Pet < 's , S > >");
    assert_eq!(shape(&named("Count")), "shape :: Plain < Count >");

    // Projections are read through a shape; models implement their own traits.
    assert!(family.view(&named("Tags"), false).is_some());
    assert!(family.view(&named("Pet"), true).is_none());
    assert_eq!(
        family.seed(&named("Pet"), true).unwrap().to_string(),
        "shape :: Optional < shape :: Model < Pet < 's , S > > >"
    );
    assert!(family.seed(&named("Count"), false).is_none());
}

#[test]
fn input_builder_arguments_convert_strings_only() {
    let family = Family::new(&family_api());
    assert_eq!(
        input_builder_arg_type(&family, &TypeRef::String).to_string(),
        "impl Into < S :: Text < 's > >"
    );
    assert_eq!(
        input_builder_arg_type(&family, &TypeRef::Integer(IntegerType::I32)).to_string(),
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
    let family = Family::new(&family_api());
    let conversion = |required, ty| {
        request_from_parts_expr(&family, &operation_with_body(required, ty))
            .to_token_stream()
            .to_string()
    };
    assert_eq!(
        conversion(None, named("Pet")),
        "satay_runtime :: into_empty_request (parts)"
    );
    assert_eq!(
        conversion(Some(true), named("Pet")),
        "satay_runtime :: into_json_request (parts)"
    );
    assert_eq!(
        conversion(Some(false), named("Pet")),
        "satay_runtime :: into_optional_json_request (parts)"
    );
    // Bodies that are bare projections serialize through their shape.
    assert_eq!(
        conversion(Some(true), named("Tags")),
        "shape :: into_json_request :: < shape :: Contiguous < shape :: Text > , S > (parts)"
    );
    assert_eq!(
        conversion(Some(false), TypeRef::option(TypeRef::String)),
        "shape :: into_optional_json_request :: < shape :: Text , S > (parts)"
    );
}

fn operation_with_body(required: Option<bool>, ty: TypeRef) -> Operation {
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
            ty: ty.clone(),
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
