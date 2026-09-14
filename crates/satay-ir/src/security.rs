//! Owned security contract records.
//!
//! Scheme names are semantic wire names, not definition-arena IDs. Requirement
//! entries retain those names and ordered scopes; alternatives are OR, entries
//! within an alternative are AND. No auth-capability validation occurs during
//! graph finalization.

/// One declared security scheme.
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityScheme {
    /// Semantic scheme name on the wire.
    pub name: String,
    /// Human-readable scheme description.
    pub description: Option<String>,
    /// Scheme mechanism.
    pub kind: SecuritySchemeKind,
}

/// Security scheme kinds accepted by the contract.
#[derive(Debug, Clone, PartialEq)]
pub enum SecuritySchemeKind {
    /// A key carried in a named location.
    ApiKey {
        /// Key name on the wire.
        wire_name: String,
        /// Where the key travels.
        location: ApiKeyLocation,
    },
    /// An HTTP authorization header.
    Http {
        /// Authorization scheme as declared.
        scheme: String,
        /// Declared bearer token format hint.
        bearer_format: Option<String>,
    },
    /// `OAuth2` with its declared flows.
    OAuth2 {
        /// Flows in declared order.
        flows: Vec<OAuthFlow>,
    },
    /// An `OpenID` Connect discovery URL.
    OpenIdConnect {
        /// Discovery URL as declared.
        url: String,
    },
    /// Mutual TLS client certificates.
    MutualTls,
}

/// Where an API key travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeyLocation {
    /// A URL query parameter.
    Query,
    /// A request header.
    Header,
    /// A cookie value.
    Cookie,
}

/// One declared `OAuth2` flow.
#[derive(Debug, Clone, PartialEq)]
pub struct OAuthFlow {
    /// Flow kind.
    pub kind: OAuthFlowKind,
    /// Declared authorization URL.
    pub authorization_url: Option<String>,
    /// Declared token URL.
    pub token_url: Option<String>,
    /// Declared refresh URL.
    pub refresh_url: Option<String>,
    /// Declared scopes in declared order.
    pub scopes: Vec<OAuthScope>,
}

/// `OAuth2` flow kinds accepted by the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthFlowKind {
    /// Redirect-based flow without a token endpoint exchange.
    Implicit,
    /// Resource-owner password credentials flow.
    Password,
    /// Client credentials flow.
    ClientCredentials,
    /// Authorization code flow with a token exchange.
    AuthorizationCode,
}

/// One declared `OAuth2` scope.
#[derive(Debug, Clone, PartialEq)]
pub struct OAuthScope {
    /// Scope name on the wire.
    pub name: String,
    /// Human-readable scope description.
    pub description: String,
}

/// One security requirement alternative.
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityRequirement {
    /// Schemes required together (AND) for this alternative.
    pub schemes: Vec<SecurityRequirementScheme>,
}

/// One scheme inside a security requirement.
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityRequirementScheme {
    /// Name of the referenced security scheme.
    pub scheme: String,
    /// Required scope names in declared order.
    pub scopes: Vec<String>,
}
