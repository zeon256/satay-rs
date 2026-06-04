use proc_macro2::Literal;
use syn::parse_quote;

use crate::model::{
    Operation, Parameter, ParameterLocation, ParseAs, PathSegment, TypeRef, is_array_type,
};

use super::super::{doc_attrs, ident, input_field, lit_str, rust_field_type};

pub(super) fn render_parts_function(operation: &Operation) -> syn::ItemFn {
    let docs = doc_attrs(operation.description.as_deref());
    let body_type = operation.request_body.as_ref().map_or_else(
        || parse_quote!(()),
        |body| rust_field_type(&body.ty, body.required, false),
    );
    let parts_fn = ident(&format!("{}_parts", operation.fn_name));
    let input_name = ident(if operation_uses_input(operation) {
        "input"
    } else {
        "_input"
    });
    let input_type = ident(&operation.input_name);
    let path_capacity = Literal::usize_unsuffixed(operation.path.len());

    let mut statements = vec![parse_quote!(let mut uri = String::with_capacity(#path_capacity);)];
    statements.extend(render_path(operation));
    statements.extend(render_query(operation));
    statements.extend(render_header_statements(operation));
    let request_parts = render_request_parts_return(operation);

    parse_quote!(
        #(#docs)*
        pub fn #parts_fn(
            #input_name: #input_type,
        ) -> Result<satay_runtime::RequestParts<#body_type>, satay_runtime::Error> {
            #(#statements)*
            #request_parts
        }
    )
}

fn render_header_statements(operation: &Operation) -> Vec<syn::Stmt> {
    let header_parameters = operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == ParameterLocation::Header)
        .collect::<Vec<_>>();
    let needs_mutable = operation.request_body.is_some() || !header_parameters.is_empty();

    let mut statements = if needs_mutable {
        vec![parse_quote!(let mut headers = http::HeaderMap::new();)]
    } else {
        return vec![parse_quote!(let headers = http::HeaderMap::new();)];
    };

    if let Some(body) = &operation.request_body {
        let content_type = lit_str(&body.content_type);
        let insert: syn::Stmt = parse_quote!(
            headers.insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static(#content_type),
            );
        );
        if body.required {
            statements.push(insert);
        } else {
            let field = ident(&body.field_name);
            statements.push(parse_quote!(
                if input.#field.is_some() {
                    #insert
                }
            ));
        }
    }

    for parameter in header_parameters {
        let wire_name = lit_str(&parameter.wire_name);
        let field = ident(&parameter.rust_name);
        let expr = value_expr(input_field(&parameter.rust_name), &parameter.ty);
        if parameter.required {
            statements.push(parse_quote!(
                satay_runtime::insert_header(&mut headers, #wire_name, #expr)?;
            ));
        } else {
            let expr = value_expr(parse_quote!(value), &parameter.ty);
            statements.push(parse_quote!(
                if let Some(value) = &input.#field {
                    satay_runtime::insert_header(&mut headers, #wire_name, #expr)?;
                }
            ));
        }
    }

    statements
}

fn render_request_parts_return(operation: &Operation) -> syn::Expr {
    let method = ident(operation.method.rust_const());
    let body = match &operation.request_body {
        Some(body) => {
            let field = ident(&body.field_name);
            quote::quote!(input.#field)
        }
        None => quote::quote!(()),
    };

    parse_quote!(
        Ok(satay_runtime::RequestParts {
            method: http::Method::#method,
            uri,
            headers,
            body: #body,
        })
    )
}

fn render_path(operation: &Operation) -> Vec<syn::Stmt> {
    let mut statements = vec![];
    for segment in &operation.path_segments {
        match segment {
            PathSegment::Literal(literal) if !literal.is_empty() => {
                let literal = lit_str(literal);
                statements.push(parse_quote!(uri.push_str(#literal);));
            }
            PathSegment::Literal(_) => {}
            PathSegment::Parameter(name) => {
                let parameter = operation
                    .parameters
                    .iter()
                    .find(|parameter| {
                        parameter.location == ParameterLocation::Path
                            && parameter.wire_name == *name
                    })
                    .expect("path parameters validated before render");
                let expr = value_expr(input_field(&parameter.rust_name), &parameter.ty);
                statements.push(parse_quote!(
                    satay_runtime::append_path_segment(&mut uri, #expr);
                ));
            }
        }
    }
    statements
}

fn render_query(operation: &Operation) -> Vec<syn::Stmt> {
    let query_parameters = operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == ParameterLocation::Query)
        .collect::<Vec<_>>();
    if query_parameters.is_empty() {
        return vec![];
    }

    let mut statements = vec![parse_quote!(let mut first_query = true;)];
    for parameter in query_parameters {
        statements.extend(render_query_parameter(parameter));
    }
    statements
}

fn render_query_parameter(parameter: &Parameter) -> Vec<syn::Stmt> {
    if let Some(item) = array_item_type(parameter.ty.non_option()) {
        return render_array_query_parameter(parameter, item);
    }

    let wire_name = lit_str(&parameter.wire_name);
    if parameter.required {
        let expr = value_expr(input_field(&parameter.rust_name), &parameter.ty);
        vec![parse_quote!(
            satay_runtime::append_query_pair(&mut uri, &mut first_query, #wire_name, #expr);
        )]
    } else {
        let field = ident(&parameter.rust_name);
        let expr = value_expr(parse_quote!(value), &parameter.ty);
        vec![parse_quote!(
            if let Some(value) = &input.#field {
                satay_runtime::append_query_pair(&mut uri, &mut first_query, #wire_name, #expr);
            }
        )]
    }
}

fn render_array_query_parameter(parameter: &Parameter, item: &TypeRef) -> Vec<syn::Stmt> {
    let wire_name = lit_str(&parameter.wire_name);
    let value = value_expr(parse_quote!(value), item);

    if parameter.required {
        let values = array_values_expr(
            input_field(&parameter.rust_name),
            &parameter.ty,
            ArrayValueBase::Owned,
        );
        vec![parse_quote!(
            for value in #values {
                satay_runtime::append_query_pair(&mut uri, &mut first_query, #wire_name, #value);
            }
        )]
    } else {
        let field = ident(&parameter.rust_name);
        let values = array_values_expr(
            parse_quote!(values),
            &parameter.ty,
            ArrayValueBase::Borrowed,
        );
        vec![parse_quote!(
            if let Some(values) = &input.#field {
                for value in #values {
                    satay_runtime::append_query_pair(&mut uri, &mut first_query, #wire_name, #value);
                }
            }
        )]
    }
}

fn array_item_type(ty: &TypeRef) -> Option<&TypeRef> {
    match ty {
        TypeRef::Array(item) => Some(item),
        TypeRef::Constrained { inner, .. } => array_item_type(inner.non_option()),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum ArrayValueBase {
    Owned,
    Borrowed,
}

fn array_values_expr(base: syn::Expr, ty: &TypeRef, base_kind: ArrayValueBase) -> syn::Expr {
    match ty.non_option() {
        TypeRef::Array(_) => match base_kind {
            ArrayValueBase::Owned => parse_quote!(&#base),
            ArrayValueBase::Borrowed => base,
        },
        TypeRef::Constrained { inner, .. } if is_array_type(inner.non_option()) => {
            parse_quote!(#base.as_ref())
        }
        _ => unreachable!("array values are only rendered for array types"),
    }
}

fn value_expr(base: syn::Expr, ty: &TypeRef) -> syn::Expr {
    match ty.non_option() {
        TypeRef::String => parse_quote!(#base.as_str()),
        TypeRef::ParsedString(parse_as) | TypeRef::ParsedInteger(parse_as) => {
            parsed_value_expr(base, *parse_as)
        }
        TypeRef::Named(_) => parse_quote!(#base.as_ref()),
        TypeRef::Range(_) => parse_quote!(&#base.to_string()),
        TypeRef::Constrained { inner, .. } => constrained_value_expr(base, inner.non_option()),
        TypeRef::Integer(_) | TypeRef::F32 | TypeRef::F64 | TypeRef::Bool => {
            parse_quote!(&#base.to_string())
        }
        TypeRef::Array(_) | TypeRef::Option(_) => unreachable!("arrays are handled by caller"),
    }
}

fn constrained_value_expr(base: syn::Expr, inner: &TypeRef) -> syn::Expr {
    match inner {
        TypeRef::String => parse_quote!(#base.as_ref()),
        TypeRef::Named(_) => parse_quote!(#base.as_ref()),
        TypeRef::Range(_) => parse_quote!(&#base.to_string()),
        TypeRef::ParsedString(parse_as) | TypeRef::ParsedInteger(parse_as) => {
            parsed_value_expr(base, *parse_as)
        }
        TypeRef::Integer(_) | TypeRef::F32 | TypeRef::F64 | TypeRef::Bool => {
            parse_quote!(&#base.to_string())
        }
        TypeRef::Array(_) | TypeRef::Constrained { .. } | TypeRef::Option(_) => {
            unreachable!("arrays are handled by caller")
        }
    }
}

fn parsed_value_expr(base: syn::Expr, parse_as: ParseAs) -> syn::Expr {
    match parse_as {
        ParseAs::Date => parse_quote!(&satay_runtime::format_date(#base)),
        ParseAs::NaiveDateTime => {
            parse_quote!(&satay_runtime::format_naive_datetime(#base))
        }
        ParseAs::OffsetDateTime => {
            parse_quote!(&satay_runtime::format_offset_datetime(#base))
        }
        ParseAs::Time => parse_quote!(&satay_runtime::format_time(#base)),
        ParseAs::U8
        | ParseAs::U16
        | ParseAs::U32
        | ParseAs::U64
        | ParseAs::I8
        | ParseAs::I16
        | ParseAs::I32
        | ParseAs::I64
        | ParseAs::F32
        | ParseAs::F64 => parse_quote!(&#base.to_string()),
        ParseAs::Bool => parse_quote!(satay_runtime::format_bool(&#base)),
        ParseAs::IntegerRange | ParseAs::NumberRange => {
            unreachable!("range parse-as uses generated range types")
        }
    }
}

fn operation_uses_input(operation: &Operation) -> bool {
    !operation.parameters.is_empty() || operation.request_body.is_some()
}
