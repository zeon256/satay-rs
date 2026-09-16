use crate::parse;
use crate::parse::parity;
use crate::parse::parity::codegen;
use crate::{GenerateOptions, RootModule};
use std::{fs, path::Path};

#[test]
fn lowers_a_hand_built_graph_without_source_input() {
    use satay_ir::{ApiBuilder, Definition, SchemaUse, StringSchema, TypeExpr};
    let mut builder = ApiBuilder::new();
    builder.add_definition(Definition {
        source_name: "Label".to_owned(),
        schema: SchemaUse::new(TypeExpr::String(StringSchema::default())),
    });
    let api = builder.finish().unwrap();
    for root_module in [RootModule::ModRs, RootModule::LibRs] {
        let options = GenerateOptions { root_module };
        let actual = super::lower_api(&api, options).unwrap();
        let expected = crate::generate_with(
            "openapi: 3.1.0\ninfo: {title: Manual, version: '1'}\npaths: {}\ncomponents:\n  schemas:\n    Label: {type: string}\n",
            options,
        ).unwrap();
        assert!(
            actual
                .iter()
                .map(|file| (&file.relative_path, &file.contents))
                .eq(expected
                    .iter()
                    .map(|file| (&file.relative_path, &file.contents)))
        );
    }
}

#[test]
fn inferred_operation_diagnostic_context() {
    for responses in ["", "      responses:\n        oops: {description: bad}\n"] {
        let spec = format!(
            "openapi: 3.1.0\ninfo: {{title: Context, version: '1'}}\npaths:\n  /first:\n    get:\n      description: First\n{responses}"
        );
        assert_eq!(
            staged(&spec, GenerateOptions::default()).unwrap_err(),
            crate::generate(&spec).unwrap_err().to_string()
        );
    }
}

fn staged(spec: &str, options: GenerateOptions) -> Result<Vec<crate::GeneratedFile>, String> {
    codegen::generate_with(spec, options).map_err(|error| error.to_string())
}

#[test]
fn diagnostic_order_across_stages() {
    for (first, second) in [
        (
            "{type: integer, format: custom}",
            "{type: string, minLength: 5, maxLength: 2}",
        ),
        (
            "{type: string, minLength: 5, maxLength: 2}",
            "{type: integer, format: custom}",
        ),
    ] {
        let spec = format!(
            "openapi: 3.1.0\ninfo: {{title: Ordering, version: '1'}}\npaths: {{}}\ncomponents:\n  schemas:\n    First: {first}\n    Second: {second}\n"
        );
        assert_eq!(
            staged(&spec, GenerateOptions::default()).unwrap_err(),
            crate::generate(&spec).unwrap_err().to_string()
        );
        let spec = format!(
            "openapi: 3.1.0\ninfo: {{title: Ordering, version: '1'}}\npaths: {{}}\ncomponents:\n  schemas:\n    Record:\n      type: object\n      properties:\n        first: {first}\n        second: {second}\n"
        );
        assert_eq!(
            staged(&spec, GenerateOptions::default()).unwrap_err(),
            crate::generate(&spec).unwrap_err().to_string()
        );
    }
    for spec in [
        "openapi: 3.1.0\ninfo: {title: Ordering, version: '1'}\ncomponents:\n  schemas:\n    First: {type: integer, format: custom}\n",
        "openapi: 3.1.0\ninfo: {title: Ordering, version: '1'}\npaths: {}\ncomponents:\n  schemas:\n    First: {type: integer, format: custom, minimum: 5, maximum: 2}\n",
        "openapi: 3.1.0\ninfo: {title: Ordering, version: '1'}\npaths: {}\ncomponents:\n  schemas:\n    First: {type: number, format: custom, minimum: 5, maximum: 2}\n",
        "openapi: 3.1.0\ninfo: {title: Ordering, version: '1'}\npaths:\n  /first:\n    get:\n      parameters:\n        - {name: first, in: query, schema: {type: integer, format: custom}}\n      responses:\n        oops: {description: bad}\n",
        "openapi: 3.1.0\ninfo: {title: Ordering, version: '1'}\npaths:\n  /first:\n    get:\n      parameters:\n        - {name: first, in: query, schema: {type: integer, format: custom}}\n",
    ] {
        assert_eq!(
            staged(spec, GenerateOptions::default()).unwrap_err(),
            crate::generate(spec).unwrap_err().to_string(),
            "{spec}"
        );
    }
}

#[test]
fn rust_source_corpus_parity() {
    use syn::visit_mut::VisitMut;
    #[derive(Default)]
    struct Specs(Vec<String>);
    impl VisitMut for Specs {
        fn visit_lit_str_mut(&mut self, literal: &mut syn::LitStr) {
            let value = literal.value();
            if value.contains("openapi:") {
                self.0.push(value);
            }
        }
    }
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/generate");
    let mut paths = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect::<Vec<_>>();
    paths.sort();
    let mut failures = vec![];
    let mut checked = 0;
    for path in paths {
        let source = fs::read_to_string(&path).unwrap();
        let mut syntax = syn::parse_file(&source).unwrap();
        let mut specs = Specs::default();
        specs.visit_file_mut(&mut syntax);
        for (index, spec) in specs.0.iter().enumerate() {
            // Format templates are not concrete specifications.
            if parse::parse_document(spec).is_err() {
                continue;
            }
            checked += 1;
            for root_module in [RootModule::ModRs, RootModule::LibRs] {
                let options = GenerateOptions { root_module };
                let old = crate::generate_with(spec, options).map_err(|error| error.to_string());
                let new = staged(spec, options);
                let equal = match (&old, &new) {
                    (Ok(old), Ok(new)) => old
                        .iter()
                        .map(|file| (&file.relative_path, &file.contents))
                        .eq(new.iter().map(|file| (&file.relative_path, &file.contents))),
                    (Err(old), Err(new)) => old == new,
                    _ => false,
                };
                if !equal {
                    failures.push(format!(
                        "{} literal {index}: legacy: {}; semantic: {}",
                        path.display(),
                        old.err().unwrap_or_else(|| "generated".to_owned()),
                        new.err()
                            .unwrap_or_else(|| "generated (contents differ)".to_owned())
                    ));
                    break;
                }
            }
        }
    }
    assert!(checked > 50, "only {checked} generation inputs checked");
    assert!(
        failures.is_empty(),
        "{} of {checked} inputs differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn fixture_file_parity() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let mut paths = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext == "yaml" || ext == "json")
        })
        .collect::<Vec<_>>();
    paths.sort();
    assert!(!paths.is_empty());
    for path in paths {
        let source = fs::read_to_string(&path).unwrap();
        parity::assert_generation(&source);
    }
}

#[test]
fn all_of_aliases_and_ignored_duplicates() {
    for schemas in [
        "Combined: {allOf: [{$ref: '#/components/schemas/Alias'}]}\n    Alias: {$ref: '#/components/schemas/Base'}\n    Base: {type: object, properties: {value: {type: string}}}",
        "Combined: {allOf: [{$ref: '#/components/schemas/Nested'}, {type: object, properties: {value: {type: string}}}]}\n    Nested: {allOf: [{$ref: '#/components/schemas/Base'}]}\n    Base: {type: object, properties: {value: {type: string, x-satay: {ignore: true}}}}",
    ] {
        let spec = format!(
            "openapi: 3.1.0\ninfo: {{title: AllOf, version: '1'}}\npaths: {{}}\ncomponents:\n  schemas:\n    {schemas}\n"
        );
        let old = crate::generate(&spec).map_err(|error| error.to_string());
        let new = staged(&spec, GenerateOptions::default());
        match (old, new) {
            (Ok(old), Ok(new)) => assert!(
                old.iter()
                    .map(|f| (&f.relative_path, &f.contents))
                    .eq(new.iter().map(|f| (&f.relative_path, &f.contents)))
            ),
            (Err(old), Err(new)) => assert_eq!(old, new),
            (old, new) => panic!("legacy: {:?}, semantic: {:?}", old.err(), new.err()),
        }
    }
}
