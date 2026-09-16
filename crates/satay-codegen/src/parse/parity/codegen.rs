//! Test-only adapter. Always returns the semantic result after comparison.
use crate::parse::{
    diagnostic,
    normalize::{NormalizeError, normalize_for_rust},
    rust::{self, LowerError},
};
pub(super) use crate::{Error, GenerateOptions, GeneratedFile, RootModule, ValidationError};

pub(in crate::parse) fn semantic_generate(
    spec: &str,
    options: GenerateOptions,
) -> Result<Vec<GeneratedFile>, Error> {
    let api = normalize_for_rust(spec, "parity.yaml").map_err(|error| match error {
        NormalizeError::Parse(error) => Error::Parse(error),
        NormalizeError::Validation { source, .. } => Error::Validation(*source),
        error => {
            panic!("semantic normalization failed outside the compatibility contract: {error:?}")
        }
    })?;
    rust::lower_api(&api, options).map_err(|error| match error {
        LowerError::Rust(error) => Error::Validation(error),
        LowerError::Frontend(error) => Error::Validation(diagnostic::restore(error)),
    })
}

pub(super) fn generate(spec: &str) -> Result<Vec<GeneratedFile>, Error> {
    generate_with(spec, GenerateOptions::default())
}

pub(in crate::parse) fn generate_with(
    spec: &str,
    options: GenerateOptions,
) -> Result<Vec<GeneratedFile>, Error> {
    // Check both layouts even for runtime-built inputs and rejection cases.
    let other = match options.root_module {
        RootModule::ModRs => RootModule::LibRs,
        RootModule::LibRs => RootModule::ModRs,
    };
    let _ = compare(spec, GenerateOptions { root_module: other });
    compare(spec, options)
}

fn compare(spec: &str, options: GenerateOptions) -> Result<Vec<GeneratedFile>, Error> {
    let legacy = crate::generate_with(spec, options);
    let semantic = semantic_generate(spec, options);
    match (&legacy, &semantic) {
        (Ok(legacy), Ok(semantic)) => {
            assert_eq!(
                legacy.len(),
                semantic.len(),
                "file count: {:?}\n{spec}",
                options.root_module
            );
            for (old, new) in legacy.iter().zip(semantic) {
                assert_eq!(
                    old.relative_path, new.relative_path,
                    "file order: {:?}\n{spec}",
                    options.root_module
                );
                assert_eq!(
                    old.contents, new.contents,
                    "contents of {}: {:?}\n{spec}",
                    old.relative_path, options.root_module
                );
            }
        }
        (Err(legacy), Err(semantic)) => {
            assert_eq!(
                legacy.to_string(),
                semantic.to_string(),
                "diagnostic: {:?}\n{spec}",
                options.root_module
            );
            // Includes enum variants, fields, nested errors, and parser positions.
            assert_eq!(
                format!("{legacy:?}"),
                format!("{semantic:?}"),
                "typed diagnostic: {:?}\n{spec}",
                options.root_module
            );
        }
        _ => panic!(
            "acceptance differs: {:?}\nlegacy: {legacy:?}\nsemantic: {semantic:?}\n{spec}",
            options.root_module
        ),
    }
    semantic
}
