/// Provenance for one schema use.
///
/// This type records caller-provided provenance; it does not parse or resolve
/// either field. The frontend is responsible for meaningful document IDs and
/// correctly encoded JSON Pointer tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRef {
    /// Caller-provided identifier for the source document.
    pub document: String,
    /// RFC 6901 JSON Pointer without a leading `#`.
    ///
    /// An empty string denotes the document root.
    pub pointer: String,
}
