use crate::{GenerateOptions, generate};

fn response_api(response: satay_ir::Response) -> satay_ir::Api {
    use satay_ir::{ApiBuilder, HttpApi, HttpMethod, Operation, OperationInterpretation, PathItem};

    let mut builder = ApiBuilder::new();
    builder.set_http(HttpApi {
        paths: vec![PathItem {
            path: "/probe".to_owned(),
            parameters: vec![],
            operations: vec![Operation {
                responses_diagnostic: None,
                source_id: Some("probe".to_owned()),
                method: HttpMethod::Get,
                description: None,
                tags: vec![],
                parameters: vec![],
                request_body: None,
                responses: vec![response],
                servers: None,
                security: None,
                interpretation: OperationInterpretation::default(),
                source: None,
            }],
            servers: None,
            source: None,
        }],
        ..Default::default()
    });
    builder.finish().expect("graph integrity is valid")
}

#[test]
fn validates_all_response_range_classes_through_the_entry_point() {
    use crate::{Error, ValidationError};
    use satay_ir::{Response, ResponseStatus};

    for class in u8::MIN..=u8::MAX {
        let api = response_api(Response {
            status: ResponseStatus::Range(class),
            description: None,
            content: vec![],
            source: None,
        });
        let result = generate(&api, GenerateOptions::default());
        if (1..=5).contains(&class) {
            result.expect("valid wildcard status class generates");
        } else {
            let Error::Rust(ValidationError::OutOfRangeStatusClass {
                context,
                class: actual,
            }) = result.expect_err("invalid wildcard status class must be rejected")
            else {
                panic!("expected structured status class error");
            };
            assert_eq!(actual, class);
            assert_eq!(context, "operation `probe` responses");
        }
    }
}

fn mapped_response(output: satay_ir::SchemaUse, required: bool) -> satay_ir::Response {
    use satay_ir::{
        MediaType, OutputSelector, Response, ResponseMediaType, ResponseProjection, ResponseStatus,
    };

    Response {
        status: ResponseStatus::Exact(200),
        description: None,
        content: vec![ResponseMediaType {
            media: MediaType {
                media_type: "application/json".to_owned(),
                schema: None,
                source: None,
            },
            projection: Some(ResponseProjection {
                unwrap_required: true,
                map_required: Some(required),
                selector: OutputSelector {
                    unwrap_field: "values".to_owned(),
                    map_field: Some("value".to_owned()),
                },
                output,
            }),
        }],
        source: None,
    }
}

#[test]
fn rejects_non_array_mapped_projections_through_the_entry_point() {
    use crate::{Error, ValidationError};
    use satay_ir::{SchemaUse, TypeExpr};

    for required in [false, true] {
        let api = response_api(mapped_response(SchemaUse::new(TypeExpr::Boolean), required));
        let Error::Rust(ValidationError::MappedResponseProjectionRequiresArray { context }) =
            generate(&api, GenerateOptions::default())
                .expect_err("a mapped scalar projection must be rejected")
        else {
            panic!("expected structured projection error");
        };
        assert_eq!(context, "operation `probe` responses 200 schema");
    }
}

#[test]
fn accepts_array_mapped_projections_through_the_entry_point() {
    use satay_ir::{ArrayConstraints, ArraySchema, SchemaUse, TypeExpr};

    for required in [false, true] {
        let output = SchemaUse::new(TypeExpr::Array(ArraySchema {
            items: Box::new(SchemaUse::new(TypeExpr::Boolean)),
            constraints: ArrayConstraints::default(),
        }));
        let api = response_api(mapped_response(output, required));
        generate(&api, GenerateOptions::default()).expect("a mapped array projection generates");
    }
}

#[test]
fn generates_from_a_hand_built_graph_without_source_input() {
    use satay_ir::{ApiBuilder, Definition, SchemaUse, StringSchema, TypeExpr};

    let mut builder = ApiBuilder::new();
    builder.add_definition(Definition {
        source_name: "Label".to_owned(),
        schema: SchemaUse::new(TypeExpr::String(StringSchema::default())),
    });
    let api = builder
        .finish()
        .expect("hand-built semantic graph is valid");
    let files = generate(&api, GenerateOptions::default())
        .expect("semantic graph generates without OpenAPI input");

    assert!(files.iter().any(|file| file.relative_path == "mod.rs"));
    assert!(files.iter().any(|file| file.relative_path == "types.rs"));
    let types = files
        .iter()
        .find(|file| file.relative_path == "types.rs")
        .expect("types file is generated");
    assert!(types.contents.contains("pub type Label"));
}

#[test]
fn surfaces_a_retained_graph_diagnostic_through_the_entry_point() {
    use crate::Error;
    use satay_ir::{ApiBuilder, Definition, Diagnostic, DiagnosticKind, SchemaUse, TypeExpr};

    let mut builder = ApiBuilder::new();
    builder.add_definition(Definition {
        source_name: "Broken".to_owned(),
        schema: SchemaUse::new(TypeExpr::Invalid(Diagnostic {
            kind: DiagnosticKind::InvalidExtension {
                context: "schema `Broken`".to_owned(),
                path: "x-satay.ignore".to_owned(),
                source: "invalid type: string, expected a boolean".to_owned(),
            },
            message: "schema `Broken` carries an invalid extension".to_owned(),
        })),
    });
    let api = builder
        .finish()
        .expect("hand-built semantic graph is valid");

    let error = generate(&api, GenerateOptions::default())
        .expect_err("a retained diagnostic must surface as Frontend");
    match error {
        Error::Frontend(diagnostic) => {
            assert_eq!(
                diagnostic.message,
                "schema `Broken` carries an invalid extension"
            );
        }
        Error::Rust(_) => panic!("expected Frontend diagnostic, got Rust error"),
    }
}
