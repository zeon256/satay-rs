use crate::model::ApiGroup;

use super::*;

#[test]
fn lowers_tags_to_ordered_api_groups_and_shortens_local_method_names() {
    let api = parse_valid(
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

    assert_eq!(api.operations[0].tags, ["bus", "realtime"]);
    assert_eq!(api.operations[1].tags, ["bus"]);
    assert!(api.operations[3].tags.is_empty());

    assert_eq!(
        api.groups
            .iter()
            .map(|group| group.rust_name.as_str())
            .collect::<Vec<_>>(),
        ["realtime", "bus", "untagged"]
    );

    let realtime = group(&api, "realtime");
    assert_eq!(realtime.wire_name.as_deref(), Some("realtime"));
    assert_eq!(realtime.description.as_deref(), Some("Realtime views."));
    assert_eq!(group_methods(realtime), ["get_bus_arrival"]);

    let bus = group(&api, "bus");
    assert_eq!(bus.description.as_deref(), Some("Bus operations."));
    assert_eq!(
        group_methods(bus),
        ["get_arrival", "list_stops", "get_arrival_2"]
    );

    let untagged = group(&api, "untagged");
    assert_eq!(untagged.wire_name, None);
    assert_eq!(group_methods(untagged), ["health"]);
}

#[test]
fn group_names_avoid_root_module_and_api_method_collisions() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Group collisions
  version: 1.0.0
tags:
  - name: api
  - name: base-url
  - name: get-user
  - name: bus-service
  - name: bus_service
paths:
  /user:
    get:
      operationId: getUser
      tags: [api, base-url, get-user, bus-service, bus_service]
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

    assert_eq!(
        api.groups
            .iter()
            .map(|group| group.rust_name.as_str())
            .collect::<Vec<_>>(),
        [
            "api_2",
            "base_url_2",
            "get_user_2",
            "bus_service",
            "bus_service_2",
            "untagged_2",
        ]
    );
}

#[test]
fn resanitizes_operation_names_after_removing_the_group_name() {
    let api = parse_valid(
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

    assert_eq!(group_methods(group(&api, "get")), ["type_"]);
}

fn group<'a>(api: &'a Api, rust_name: &str) -> &'a ApiGroup {
    api.groups
        .iter()
        .find(|group| group.rust_name == rust_name)
        .unwrap_or_else(|| panic!("missing API group {rust_name}"))
}

fn group_methods(group: &ApiGroup) -> Vec<&str> {
    group
        .operations
        .iter()
        .map(|operation| operation.method_name.as_str())
        .collect()
}
