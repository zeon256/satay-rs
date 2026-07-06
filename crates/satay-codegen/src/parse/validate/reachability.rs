//! Reachability-aware exclusion of `components.schemas` entries.
//!
//! `x-satay.skip` removes an operation from generation, but component schemas
//! reachable *only* from skipped operations would otherwise still be validated
//! and lowered, so a skipped multipart/binary operation could still fail the
//! whole spec. This module computes the set of component schemas to exclude:
//! everything reachable only from skipped operations (and from skipped path
//! parameters), transitively — while keeping every schema reachable from a
//! retained operation and every schema referenced by no operation at all.

use std::collections::BTreeSet;

use oas3::Map as OasMap;
use oas3::spec::{
    MediaType as OasMediaType, ObjectOrReference, Operation as OasOperation,
    Parameter as OasParameter, Schema as OasSchema,
};

use super::super::helpers::json_media_type;
use super::super::reference::{
    resolve_parameter, resolve_path_item, resolve_request_body, resolve_response,
};
use super::super::resolve::{ResolvedDocument, refs::local_ref_name};
use super::operation::{inferred_operation_id, operation_satay_skip};
use crate::error::ValidationError;
use crate::model::HttpMethod;

/// Which media types of a content map contribute schema references.
#[derive(Debug, Clone, Copy)]
enum MediaScope {
    /// Only the JSON media type — mirrors what validation/lowering consume for
    /// retained operations, so nothing kept is wrongly dropped.
    Json,
    /// Every media type — used for skipped operations so their non-JSON
    /// (multipart/binary) component refs are excluded aggressively.
    All,
}

/// Returns the set of `components.schemas` names to EXCLUDE from validation and
/// lowering.
///
/// A schema is excluded iff it is reachable only from skipped operations (or
/// skipped path parameters) and is pulled in by neither a retained operation
/// nor an unreferenced-but-valid component schema. When no operation is
/// skipped the result is always empty, so specs without `x-satay.skip`
/// generate byte-for-byte as before.
pub(super) fn excluded_component_schemas(
    document: &ResolvedDocument<'_>,
) -> Result<BTreeSet<String>, ValidationError> {
    let Some(components) = document.spec.components.as_ref() else {
        return Ok(BTreeSet::new());
    };
    let all_names: BTreeSet<String> = components.schemas.keys().cloned().collect();

    let mut retained_seeds = BTreeSet::new();
    let mut skipped_seeds = BTreeSet::new();

    if let Some(paths) = document.spec.paths.as_ref() {
        for (path, path_item) in paths {
            let path_item = resolve_path_item(document, path_item, &format!("path item `{path}`"))?;

            let mut has_present = false;
            let mut has_retained = false;
            let mut retained_ops: Vec<&OasOperation> = vec![];
            let mut skipped_ops: Vec<&OasOperation> = vec![];

            for (method, operation) in [
                (HttpMethod::Get, path_item.get.as_ref()),
                (HttpMethod::Post, path_item.post.as_ref()),
                (HttpMethod::Put, path_item.put.as_ref()),
                (HttpMethod::Patch, path_item.patch.as_ref()),
                (HttpMethod::Delete, path_item.delete.as_ref()),
                (HttpMethod::Head, path_item.head.as_ref()),
                (HttpMethod::Options, path_item.options.as_ref()),
                (HttpMethod::Trace, path_item.trace.as_ref()),
            ] {
                let Some(operation) = operation else {
                    continue;
                };
                has_present = true;
                let operation_id = operation
                    .operation_id
                    .clone()
                    .unwrap_or_else(|| inferred_operation_id(method, path));
                if operation_satay_skip(operation, &operation_id)? {
                    skipped_ops.push(operation);
                } else {
                    retained_ops.push(operation);
                    has_retained = true;
                }
            }

            // Path parameters are kept iff step 1's guard would validate them:
            // when the path has no operations, or at least one is retained.
            let path_params_kept = !has_present || has_retained;
            let params_bucket = if path_params_kept {
                &mut retained_seeds
            } else {
                &mut skipped_seeds
            };
            for parameter in &path_item.parameters {
                collect_parameter_schema_refs(document, parameter, params_bucket)?;
            }

            for operation in retained_ops {
                collect_operation_schema_refs(
                    document,
                    operation,
                    MediaScope::Json,
                    &mut retained_seeds,
                )?;
            }
            for operation in skipped_ops {
                collect_operation_schema_refs(
                    document,
                    operation,
                    MediaScope::All,
                    &mut skipped_seeds,
                )?;
            }
        }
    }

    // No skipped-reachable schema ⇒ exclude nothing ⇒ specs without skip are
    // unchanged. The general computation below also yields ∅ here; this is an
    // explicit early return for clarity and to skip closure work.
    if skipped_seeds.is_empty() {
        return Ok(BTreeSet::new());
    }

    let mut all_seeds = retained_seeds.clone();
    all_seeds.extend(skipped_seeds.iter().cloned());
    let reachable = closure(document, all_seeds);

    // Schemas not reachable from any operation stay (unreferenced-but-valid).
    let unreferenced: BTreeSet<String> = all_names.difference(&reachable).cloned().collect();

    // Everything reachable from a retained operation, plus everything an
    // unreferenced (kept) schema references — closing over refs prevents a
    // dangling ref from a kept schema into an excluded one.
    let mut kept_roots = retained_seeds;
    kept_roots.extend(unreferenced);
    let kept = closure(document, kept_roots);

    Ok(all_names.difference(&kept).cloned().collect())
}

/// Walks one schema's inline structure, collecting the names of direct
/// `#/components/schemas/*` references (without following them). Mirrors
/// `resolve::validate_object_schema_refs` plus `additional_properties`.
fn collect_schema_ref_names(schema: &OasSchema, out: &mut BTreeSet<String>) {
    match schema {
        OasSchema::Boolean(_) => {}
        OasSchema::Object(object) => match object.as_ref() {
            ObjectOrReference::Ref { ref_path, .. } => {
                if let Ok(name) = local_ref_name(ref_path, "schemas") {
                    out.insert(name);
                }
            }
            ObjectOrReference::Object(object) => {
                for property in object.properties.values() {
                    collect_schema_ref_names(property, out);
                }
                if let Some(items) = object.items.as_deref() {
                    collect_schema_ref_names(items, out);
                }
                if let Some(additional) = object.additional_properties.as_ref() {
                    collect_schema_ref_names(additional, out);
                }
                for schema in object
                    .all_of
                    .iter()
                    .chain(&object.any_of)
                    .chain(&object.one_of)
                {
                    collect_schema_ref_names(schema, out);
                }
            }
        },
    }
}

/// Transitive closure over the component→component reference graph, starting
/// from `roots`. Cycle-safe via `seen`; refs resolve because `resolve_document`
/// already validated them.
fn closure(document: &ResolvedDocument<'_>, roots: BTreeSet<String>) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut worklist: Vec<String> = roots.into_iter().collect();

    while let Some(name) = worklist.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(schema) = document
            .spec
            .components
            .as_ref()
            .and_then(|components| components.schemas.get(&name))
        else {
            continue;
        };
        let mut edges = BTreeSet::new();
        collect_schema_ref_names(schema, &mut edges);
        for edge in edges {
            if !seen.contains(&edge) {
                worklist.push(edge);
            }
        }
    }

    seen
}

/// Collects a parameter's schema references (media-scope-independent).
fn collect_parameter_schema_refs<'a>(
    document: &ResolvedDocument<'a>,
    parameter: &'a ObjectOrReference<OasParameter>,
    out: &mut BTreeSet<String>,
) -> Result<(), ValidationError> {
    let parameter = resolve_parameter(document, parameter, "reachability parameter")?;
    if let Some(schema) = parameter.schema.as_ref() {
        collect_schema_ref_names(schema, out);
    }
    Ok(())
}

/// Collects schema references from a content map at the given media scope.
fn collect_content_schema_refs(
    content: &OasMap<String, OasMediaType>,
    scope: MediaScope,
    out: &mut BTreeSet<String>,
) {
    match scope {
        MediaScope::Json => {
            if let Some((_, media)) = json_media_type(content)
                && let Some(schema) = media.schema.as_ref()
            {
                collect_schema_ref_names(schema, out);
            }
        }
        MediaScope::All => {
            for media in content.values() {
                if let Some(schema) = media.schema.as_ref() {
                    collect_schema_ref_names(schema, out);
                }
            }
        }
    }
}

/// Collects one operation's schema-ref seeds (parameters, request body,
/// responses) at the given media scope.
fn collect_operation_schema_refs<'a>(
    document: &ResolvedDocument<'a>,
    operation: &'a OasOperation,
    scope: MediaScope,
    out: &mut BTreeSet<String>,
) -> Result<(), ValidationError> {
    for parameter in &operation.parameters {
        collect_parameter_schema_refs(document, parameter, out)?;
    }

    if let Some(request_body) = operation.request_body.as_ref() {
        let request_body =
            resolve_request_body(document, request_body, "reachability requestBody")?;
        collect_content_schema_refs(&request_body.content, scope, out);
    }

    if let Some(responses) = operation.responses.as_ref() {
        for (status, response) in responses {
            // Narrow (Json) mirrors `validate_responses`, which ignores
            // `default`; broad (All) includes it to exclude aggressively.
            if matches!(scope, MediaScope::Json) && status == "default" {
                continue;
            }
            let response = resolve_response(document, response, "reachability response")?;
            collect_content_schema_refs(&response.content, scope, out);
        }
    }

    Ok(())
}
