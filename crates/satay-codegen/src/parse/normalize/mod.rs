//! Private OpenAPI-to-`satay-ir` frontend.
//!
//! Test-gated staging: no public generation or normalization API exists yet.
//! [`normalize_spec`] runs the existing parser, resolver, and reachability
//! selection, then produces an owned, self-contained [`satay_ir::Api`].
//! Production `generate`, `generate_with`, and `parse_api` remain on the old
//! path.

mod constraint;
mod error;
mod http;
mod interpretation;
mod schema;
mod source;

pub(in crate::parse) use error::NormalizeError;

use std::collections::{BTreeMap, BTreeSet};

use super::resolve::{ResolvedDocument, resolve_document};
use super::validate::is_supported_openapi_version;
use super::validate::reachability::excluded_component_schemas;
use crate::error::ValidationError;

use satay_ir::{Api, ApiBuilder, DefinitionId, SourceRef};

use schema::SchemaPosition;

/// Normalizes an OpenAPI document into an owned semantic graph.
///
/// Resolution runs before version and selection checks, matching production's
/// reference-first ordering. Input, resolver, presence, and temporary indexes
/// are dropped before returning; only the finalized [`Api`] escapes.
///
/// # Errors
///
/// Reports parse, resolution, selection, semantic, and graph errors as a
/// structured [`NormalizeError`].
pub(in crate::parse) fn normalize_spec(
    spec: &str,
    document_id: &str,
) -> Result<Api, NormalizeError> {
    normalize(spec, document_id, false)
}

/// Retains recoverable failures at their semantic position for target validation.
pub(in crate::parse) fn normalize_for_rust(
    spec: &str,
    document_id: &str,
) -> Result<Api, NormalizeError> {
    normalize(spec, document_id, true)
}

fn normalize(spec: &str, document_id: &str, recover: bool) -> Result<Api, NormalizeError> {
    let document = super::parse_document(spec)?;
    let resolved = resolve_document(&document).map_err(|source| NormalizeError::Validation {
        location: source::source_ref(document_id, ""),
        source: Box::new(source),
    })?;
    let presence = source::PresenceIndex::read(spec)?;
    normalize_document(&resolved, document_id, &presence, recover)
}

/// Converts one resolved document with its presence index into an owned graph.
fn normalize_document<'doc>(
    document: &ResolvedDocument<'doc>,
    document_id: &str,
    presence: &source::PresenceIndex,
    recover: bool,
) -> Result<Api, NormalizeError> {
    let root = SourceRef {
        document: document_id.to_owned(),
        pointer: String::new(),
    };

    let openapi = document.spec.openapi.as_str();
    if !is_supported_openapi_version(openapi) {
        return Err(NormalizeError::Validation {
            location: root,
            source: Box::new(ValidationError::UnsupportedOpenApiVersion {
                version: openapi.to_owned(),
            }),
        });
    }

    let excluded =
        excluded_component_schemas(document).map_err(|error| NormalizeError::Validation {
            location: root.clone(),
            source: Box::new(error),
        })?;

    let mut builder = ApiBuilder::new();
    let definitions = reserve_definitions(document, &excluded, &mut builder);
    let context = NormalizeContext {
        document,
        document_id,
        presence,
        excluded: &excluded,
        definitions,
        recover,
    };

    context.define_all(&mut builder)?;
    let http = context.http()?;
    builder.set_http(http);

    builder.finish().map_err(NormalizeError::from)
}

/// Reserves one [`DefinitionId`] per non-excluded component schema, keyed by
/// the decoded original component name, in insertion order.
fn reserve_definitions<'doc>(
    document: &ResolvedDocument<'doc>,
    excluded: &BTreeSet<String>,
    builder: &mut ApiBuilder,
) -> BTreeMap<&'doc str, DefinitionId> {
    let mut definitions = BTreeMap::new();
    let Some(components) = document.spec.components.as_ref() else {
        return definitions;
    };

    for name in components.schemas.keys() {
        if excluded.contains(name) {
            continue;
        }
        let id = builder.reserve_definition();
        definitions.insert(name.as_str(), id);
    }

    definitions
}

/// Immutable conversion context shared by the schema, interpretation, and HTTP
/// converters. It borrows original source names; final IR names, descriptions,
/// and defaults are owned. Query-local cycle guards live at call sites so
/// repeated read-only schema queries cannot leak traversal state.
#[derive(Debug)]
pub(in crate::parse) struct NormalizeContext<'a, 'doc> {
    pub(in crate::parse) recover: bool,
    pub(in crate::parse) document: &'a ResolvedDocument<'doc>,
    pub(in crate::parse) document_id: &'a str,
    pub(in crate::parse) presence: &'a source::PresenceIndex,
    pub(in crate::parse) excluded: &'a BTreeSet<String>,
    pub(in crate::parse) definitions: BTreeMap<&'doc str, DefinitionId>,
}

/// Attaches the frontend location to a raw validation error.
pub(in crate::parse) trait ValidationErrorExt {
    /// Wraps the error as a [`NormalizeError::Validation`] at `location`.
    fn at(self, context: &NormalizeContext<'_, '_>, location: &str) -> NormalizeError;
}

impl ValidationErrorExt for ValidationError {
    fn at(self, context: &NormalizeContext<'_, '_>, location: &str) -> NormalizeError {
        NormalizeError::Validation {
            location: source::source_ref(context.document_id, location),
            source: Box::new(self),
        }
    }
}
