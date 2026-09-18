use super::lower_model;

#[test]
fn lowers_a_hand_built_graph_without_source_input() {
    use satay_ir::{ApiBuilder, Definition, SchemaUse, StringSchema, TypeExpr};

    let mut builder = ApiBuilder::new();
    builder.add_definition(Definition {
        source_name: "Label".to_owned(),
        schema: SchemaUse::new(TypeExpr::String(StringSchema::default())),
    });
    let api = builder
        .finish()
        .expect("hand-built semantic graph is valid");
    let model = lower_model(&api).expect("semantic graph lowers without OpenAPI input");

    assert_eq!(model.components.len(), 1);
    assert_eq!(model.components[0].rust_name, "Label");
    assert!(model.operations.is_empty());
}

#[test]
fn public_route_preserves_first_error_order() {
    let spec = "openapi: 3.1.0\ninfo: {title: Ordering, version: '1'}\npaths: {}\ncomponents:\n  schemas:\n    First: {type: integer, format: custom}\n    Second: {type: string, minLength: 5, maxLength: 2}\n";
    assert_eq!(
        crate::generate(spec).unwrap_err().to_string(),
        "schema `First` uses unsupported integer format `custom`"
    );
}
