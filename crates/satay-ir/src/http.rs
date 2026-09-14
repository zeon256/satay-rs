//! Owned target-neutral HTTP API contract.
//!
//! Records retain declared wire facts and intent without production routing
//! policies, Rust type selection, or metadata reread from an OAS document.
use crate::security::{SecurityRequirement, SecurityScheme};
use crate::{SchemaUse, SourceRef};

/// A complete owned HTTP API surface.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HttpApi {
    /// Paths in caller order.
    pub paths: Vec<PathItem>,
    /// Root servers in caller order.
    pub servers: Vec<Server>,
    /// Declared security schemes in caller order.
    pub security_schemes: Vec<SecurityScheme>,
    /// Root security alternatives.
    pub security: Vec<SecurityRequirement>,
    /// Declared tags in caller order.
    pub tags: Vec<Tag>,
}

/// One path and its shared metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct PathItem {
    /// Path template as declared on the wire.
    pub path: String,
    /// Parameters shared by operations on this path.
    pub parameters: Vec<Parameter>,
    /// Operations on this path in caller order.
    pub operations: Vec<Operation>,
    /// Servers local to this path.
    ///
    /// `None` means no local declaration; supplied empty lists are preserved.
    pub servers: Option<Vec<Server>>,
    /// Provenance of this path.
    pub source: Option<SourceRef>,
}

/// One HTTP operation.
#[derive(Debug, Clone, PartialEq)]
pub struct Operation {
    /// Declared operation ID; `None` retains an undeclared ID without
    /// embedding any inferred name.
    pub source_id: Option<String>,
    /// HTTP method.
    pub method: HttpMethod,
    /// Human-readable operation description.
    pub description: Option<String>,
    /// Tag names in declared order.
    pub tags: Vec<String>,
    /// Parameters local to this operation.
    pub parameters: Vec<Parameter>,
    /// Declared request body, when present.
    pub request_body: Option<RequestBody>,
    /// Responses in declared order.
    pub responses: Vec<Response>,
    /// Servers local to this operation.
    ///
    /// `None` means no local declaration; supplied empty lists are preserved.
    pub servers: Option<Vec<Server>>,
    /// Security alternatives local to this operation.
    ///
    /// `None` inherits the root security list, `Some(vec![])` disables it, and
    /// `Some(vec![SecurityRequirement { schemes: vec![] }])` contains an
    /// anonymous alternative.
    pub security: Option<Vec<SecurityRequirement>>,
    /// Declared operation interpretation.
    pub interpretation: OperationInterpretation,
    /// Provenance of this operation.
    pub source: Option<SourceRef>,
}

/// HTTP methods accepted by the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    /// The `GET` method.
    Get,
    /// The `POST` method.
    Post,
    /// The `PUT` method.
    Put,
    /// The `PATCH` method.
    Patch,
    /// The `DELETE` method.
    Delete,
    /// The `HEAD` method.
    Head,
    /// The `OPTIONS` method.
    Options,
    /// The `TRACE` method.
    Trace,
}

/// Operation interpretation intent.
///
/// `skip` is retained as intent; finalization does not filter skipped
/// operations or bypass integrity checking.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OperationInterpretation {
    /// Whether the operation is intentionally skipped by later targets.
    pub skip: bool,
    /// Output selection for successful responses.
    pub output: Option<OutputSelector>,
}

/// A declared output selector.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputSelector {
    /// Field unwrapped from the response envelope.
    pub unwrap_field: String,
    /// Field further mapped inside the unwrapped value.
    pub map_field: Option<String>,
}

/// One declared parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    /// Parameter name on the wire.
    pub wire_name: String,
    /// Where the parameter travels.
    pub location: ParameterLocation,
    /// Whether the parameter must be present.
    pub required: bool,
    /// Human-readable parameter description.
    pub description: Option<String>,
    /// Schema use for the parameter's value.
    pub schema: SchemaUse,
    /// Declared serialization style.
    pub style: Option<ParameterStyle>,
    /// Declared `explode` serialization flag.
    pub explode: Option<bool>,
    /// Declared `allowReserved` flag.
    pub allow_reserved: Option<bool>,
    /// Declared `allowEmptyValue` flag.
    pub allow_empty_value: Option<bool>,
    /// Provenance of this parameter.
    pub source: Option<SourceRef>,
}

/// Where a parameter travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterLocation {
    /// A path segment parameter.
    Path,
    /// A URL query parameter.
    Query,
    /// A request header.
    Header,
    /// A cookie value.
    Cookie,
}

/// Declared parameter serialization styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterStyle {
    /// Path parameters in matrix form.
    Matrix,
    /// Path parameters in label form.
    Label,
    /// Query, header, or cookie parameters in form style.
    Form,
    /// Path, query, or header parameters in simple form.
    Simple,
    /// Array values separated by spaces.
    SpaceDelimited,
    /// Array values separated by pipes.
    PipeDelimited,
    /// Objects encoded as nested query parameters.
    DeepObject,
}

/// One declared request body.
#[derive(Debug, Clone, PartialEq)]
pub struct RequestBody {
    /// Human-readable request body description.
    pub description: Option<String>,
    /// Whether the request body must be present.
    pub required: bool,
    /// Media entries in declared order.
    pub content: Vec<MediaType>,
    /// Provenance of this request body.
    pub source: Option<SourceRef>,
}

/// One request media entry.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaType {
    /// Media type string as declared.
    pub media_type: String,
    /// Schema use for the request payload, when declared.
    pub schema: Option<SchemaUse>,
    /// Provenance of this media entry.
    pub source: Option<SourceRef>,
}

/// One declared response.
#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    /// Status this response applies to.
    pub status: ResponseStatus,
    /// Human-readable response description.
    pub description: Option<String>,
    /// Response media entries in declared order.
    pub content: Vec<ResponseMediaType>,
    /// Provenance of this response.
    pub source: Option<SourceRef>,
}

/// A response status selector.
///
/// No matching precedence is baked into storage; declaration order is caller
/// order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseStatus {
    /// An exact status code.
    Exact(u16),
    /// A status range class; `2` covers 2xx.
    Range(u8),
    /// The default response.
    Default,
}

/// One response media entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ResponseMediaType {
    /// The original media entry.
    pub media: MediaType,
    /// Declared projection, when present.
    ///
    /// The projected output use is supplied by the caller; it never replaces
    /// `media.schema`.
    pub projection: Option<ResponseProjection>,
}

/// A projected response output.
#[derive(Debug, Clone, PartialEq)]
pub struct ResponseProjection {
    /// Selector producing the projected value.
    pub selector: OutputSelector,
    /// Schema use for the projected output.
    pub output: SchemaUse,
}

/// One declared server.
#[derive(Debug, Clone, PartialEq)]
pub struct Server {
    /// Server URL template as declared.
    pub url: String,
    /// Human-readable server description.
    pub description: Option<String>,
    /// Server URL variables in declared order.
    pub variables: Vec<ServerVariable>,
}

/// One server URL variable.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerVariable {
    /// Variable name as declared.
    pub name: String,
    /// Default substitution value.
    pub default: String,
    /// Allowed substitution values, when declared.
    pub enum_values: Vec<String>,
    /// Human-readable variable description.
    pub description: Option<String>,
}

/// One declared tag.
#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    /// Tag name as declared.
    pub name: String,
    /// Human-readable tag description.
    pub description: Option<String>,
}
