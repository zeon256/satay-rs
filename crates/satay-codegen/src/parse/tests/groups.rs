use super::ast::*;
use super::*;
use syn::{ImplItem, Item};

/// Extracts the names of public modules declared at the file's top level.
fn pub_mod_names(file: &syn::File) -> Vec<String> {
    file.items
        .iter()
        .filter_map(|item| match item {
            Item::Mod(item_mod) if is_pub(&item_mod.vis) => Some(item_mod.ident.to_string()),
            _ => None,
        })
        .collect()
}

/// Extracts the method names of the group `Api` view, in declaration order.
fn group_methods(file: &syn::File) -> Vec<String> {
    file.items
        .iter()
        .find_map(|item| {
            let Item::Impl(item_impl) = item else {
                return None;
            };
            (norm(&item_impl.self_ty).contains("Api")).then(|| {
                item_impl
                    .items
                    .iter()
                    .filter_map(|impl_item| match impl_item {
                        ImplItem::Fn(method) => Some(method.sig.ident.to_string()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
        })
        .unwrap_or_default()
}

/// Extracts the doc-comment lines of a top-level module declaration.
fn mod_doc_lines(file: &syn::File, name: &str) -> Vec<String> {
    file.items
        .iter()
        .find_map(|item| match item {
            Item::Mod(item_mod) if item_mod.ident == name => Some(doc_lines(&item_mod.attrs)),
            _ => None,
        })
        .unwrap_or_default()
}

#[test]
fn lowers_tags_to_ordered_api_groups_and_shortens_local_method_names() {
    let files = generate_valid(
        r#"
openapi: 3.1.0
info:
  title: Grouped API
  version: 1.0.0
tags:
  - name: realtime
    description: Realtime views.
  - name: bus
    description: Bus operations.
paths:
  /arrival:
    get:
      operationId: getBusArrival
      tags: [bus, realtime]
      responses:
        '204':
          description: No content
  /stops:
    get:
      operationId: listBusStops
      tags: [bus]
      responses:
        '204':
          description: No content
  /arrival-direct:
    get:
      operationId: getArrival
      tags: [bus]
      responses:
        '204':
          description: No content
  /health:
    get:
      operationId: health
      responses:
        '204':
          description: No content
"#,
    );

    let root = parse_rust(file(&files, "mod.rs"));
    assert_eq!(
        pub_mod_names(&root)
            .into_iter()
            .filter(|name| ["realtime", "bus", "untagged"].contains(&name.as_str()))
            .collect::<Vec<_>>(),
        ["realtime", "bus", "untagged"]
    );

    let realtime = parse_rust(file(&files, "realtime.rs"));
    assert_eq!(mod_doc_lines(&root, "realtime"), ["Realtime views."]);
    assert_eq!(group_methods(&realtime), ["get_bus_arrival"]);

    let bus = parse_rust(file(&files, "bus.rs"));
    assert_eq!(mod_doc_lines(&root, "bus"), ["Bus operations."]);
    assert_eq!(
        group_methods(&bus),
        ["get_arrival", "list_stops", "get_arrival_2"]
    );

    let untagged = parse_rust(file(&files, "untagged.rs"));
    assert_eq!(group_methods(&untagged), ["health"]);
}

#[test]
fn group_names_avoid_root_module_and_api_method_collisions() {
    let files = generate_valid(
        r#"
openapi: 3.1.0
info:
  title: Group collisions
  version: 1.0.0
tags:
  - name: api
  - name: base-url
  - name: string-storage
  - name: get-user
  - name: bus-service
  - name: bus_service
paths:
  /user:
    get:
      operationId: getUser
      tags: [api, base-url, string-storage, get-user, bus-service, bus_service]
      responses:
        '204':
          description: No content
  /untagged:
    get:
      operationId: untagged
      responses:
        '204':
          description: No content
"#,
    );

    let root = parse_rust(file(&files, "mod.rs"));
    assert_eq!(
        pub_mod_names(&root)
            .into_iter()
            .filter(|name| name != "operations")
            .collect::<Vec<_>>(),
        [
            "api_2",
            "base_url_2",
            "string_storage_2",
            "get_user_2",
            "bus_service",
            "bus_service_2",
            "untagged_2",
        ]
    );
}

#[test]
fn resanitizes_operation_names_after_removing_the_group_name() {
    let files = generate_valid(
        r#"
openapi: 3.1.0
info:
  title: Group method sanitization
  version: 1.0.0
paths:
  /type:
    get:
      operationId: getType
      tags: [get]
      responses:
        '204':
          description: No content
"#,
    );

    let get = parse_rust(file(&files, "get.rs"));
    assert_eq!(group_methods(&get), ["type_"]);
}
