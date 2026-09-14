use crate::DefinitionId;

/// How a string schema is interpreted beyond plain text.
///
/// The default, [`StringInterpretation::Plain`], means no explicit parse-as
/// interpretation was declared; formats and other annotations remain separate
/// schema facts.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum StringInterpretation {
    /// Decode as an opaque string without alternative interpretation.
    #[default]
    Plain,
    /// Decode as one of the supported scalar wire types.
    Scalar(StringScalar),
    /// Decode as a boolean through explicit wire value mappings.
    MappedBool(BoolMapping),
    /// Decode an integer encoded as a string.
    IntegerRange {
        /// Requested representation; `None` keeps no explicit width intent.
        representation: Option<IntegerRepresentation>,
    },
    /// Decode a number encoded as a string.
    NumberRange,
    /// Decode packed coordinates into the referenced definition's fields.
    Coordinates(CoordinatesInterpretation),
}

/// Scalar wire types a string may be interpreted as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringScalar {
    /// Unsigned 8-bit integer encoded as a string.
    U8,
    /// Unsigned 16-bit integer encoded as a string.
    U16,
    /// Unsigned 32-bit integer encoded as a string.
    U32,
    /// Unsigned 64-bit integer encoded as a string.
    U64,
    /// Signed 8-bit integer encoded as a string.
    I8,
    /// Signed 16-bit integer encoded as a string.
    I16,
    /// Signed 32-bit integer encoded as a string.
    I32,
    /// Signed 64-bit integer encoded as a string.
    I64,
    /// Single-precision float encoded as a string.
    F32,
    /// Double-precision float encoded as a string.
    F64,
    /// Boolean encoded as a string.
    Bool,
    /// Calendar date encoded as a string.
    Date,
    /// Timezone-less datetime encoded as a string.
    NaiveDatetime,
    /// Timezoned datetime encoded as a string.
    OffsetDatetime,
    /// Time of day encoded as a string.
    Time,
}

/// How an integer schema is interpreted beyond its numeric value.
#[derive(Debug, Clone, PartialEq)]
pub enum IntegerInterpretation {
    /// Decode as a numeric integer value.
    Numeric {
        /// Requested representation; `None` keeps no explicit width intent.
        representation: Option<IntegerRepresentation>,
    },
    /// Decode as a boolean.
    ///
    /// Carries no representation field; a bool has no integer width. This is
    /// proven by the impossible-state guarantee below.
    ///
    /// ```compile_fail
    /// use satay_ir::{IntegerInterpretation, IntegerRepresentation};
    ///
    /// // Bool is a unit variant; it has no representation field.
    /// let _bool = IntegerInterpretation::Bool {
    ///     representation: Some(IntegerRepresentation::U32),
    /// };
    /// ```
    Bool,
}

impl Default for IntegerInterpretation {
    fn default() -> Self {
        Self::Numeric {
            representation: None,
        }
    }
}

/// Explicit integer representation intent.
///
/// `Auto` records that an explicit representation was requested without
/// selecting a width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegerRepresentation {
    /// An explicit representation request with no width selected.
    Auto,
    /// Unsigned 8-bit representation.
    U8,
    /// Unsigned 16-bit representation.
    U16,
    /// Unsigned 32-bit representation.
    U32,
    /// Unsigned 64-bit representation.
    U64,
    /// Signed 8-bit representation.
    I8,
    /// Signed 16-bit representation.
    I16,
    /// Signed 32-bit representation.
    I32,
    /// Signed 64-bit representation.
    I64,
}

/// Property-local policy: whether the property participates in decoding.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyPolicy {
    /// The property is recorded but excluded from the decoded model.
    ///
    /// Retains the complete wire schema; cannot carry included-field policy.
    /// This is proven by the impossible-state guarantee below.
    ///
    /// ```compile_fail
    /// use satay_ir::{DecodePolicy, PropertyPolicy};
    ///
    /// // Ignored is a unit variant; it has no decoding field.
    /// let _ignored = PropertyPolicy::Ignored {
    ///     decoding: DecodePolicy::PropagateError,
    /// };
    /// ```
    Ignored,
    /// The property participates in the decoded model.
    Included {
        /// Explicit included-field identifier words.
        identifier: Option<Vec<String>>,
        /// How decoding failures behave for this property.
        decoding: DecodePolicy,
    },
}

impl Default for PropertyPolicy {
    fn default() -> Self {
        Self::Included {
            identifier: None,
            decoding: DecodePolicy::default(),
        }
    }
}

/// How a decoding failure on one property behaves.
///
/// `PropagateError` keeps the failure as an error; it does not constrain
/// property presence, nullability, or unknown fields.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum DecodePolicy {
    /// A decoding failure remains an error.
    #[default]
    PropagateError,
    /// A decoding failure produces absence.
    ErrorAsAbsent,
    /// Specific wire values produce absence; other failures remain errors.
    SentinelAsAbsent(SentinelValues),
}

/// One explicitly requested enum variant name.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariantName {
    /// Value as it appears on the wire.
    pub wire_value: String,
    /// Explicitly requested name, not a generated Rust name.
    pub requested_name: String,
}

/// Checked description of a packed coordinate string target.
///
/// Construction enforces the local invariants: nonempty and distinct field
/// names plus a nonempty delimiter. Whether the target's fields exist with the
/// declared shape is a semantic check outside this type.
#[derive(Debug, Clone, PartialEq)]
pub struct CoordinatesInterpretation {
    target: DefinitionId,
    fields: [String; 2],
    delimiter: String,
}

impl CoordinatesInterpretation {
    /// Creates a checked coordinates interpretation.
    ///
    /// The declared field count is fixed by the array type. Target existence
    /// and the target's actual field shape are not constructor checks.
    ///
    /// # Errors
    ///
    /// Returns `EmptyCoordinateField` when a declared field name is empty,
    /// `DuplicateCoordinateField` when the names are not distinct, or
    /// `EmptyCoordinateDelimiter` when the delimiter is empty.
    pub fn new(
        target: DefinitionId,
        fields: [String; 2],
        delimiter: String,
    ) -> Result<Self, InterpretationError> {
        if fields[0].is_empty() {
            return Err(InterpretationError::EmptyCoordinateField { index: 0 });
        }
        if fields[1].is_empty() {
            return Err(InterpretationError::EmptyCoordinateField { index: 1 });
        }
        if fields[0] == fields[1] {
            return Err(InterpretationError::DuplicateCoordinateField);
        }
        if delimiter.is_empty() {
            return Err(InterpretationError::EmptyCoordinateDelimiter);
        }

        Ok(Self {
            target,
            fields,
            delimiter,
        })
    }

    /// Definition the coordinates decode into.
    #[must_use]
    pub fn target(&self) -> DefinitionId {
        self.target
    }

    /// Ordered field names inside the target.
    #[must_use]
    pub fn fields(&self) -> &[String; 2] {
        &self.fields
    }

    /// Delimiter separating packed field values.
    #[must_use]
    pub fn delimiter(&self) -> &str {
        &self.delimiter
    }
}

/// Checked mapping of wire values onto booleans.
///
/// Construction enforces nonempty and non-overlapping value lists. List order
/// and duplicates are retained; an empty string is a valid mapped value.
#[derive(Debug, Clone, PartialEq)]
pub struct BoolMapping {
    true_values: Vec<String>,
    false_values: Vec<String>,
    unknown_as: Option<bool>,
}

impl BoolMapping {
    /// Creates a checked boolean mapping.
    ///
    /// Both lists must be nonempty and must not overlap. Values are compared
    /// without cloning the inputs.
    ///
    /// # Errors
    ///
    /// Returns `EmptyTrueValues` or `EmptyFalseValues` when a list is empty,
    /// or `OverlappingBoolValue` naming the first overlapping wire value in
    /// true-list order.
    pub fn new(
        true_values: Vec<String>,
        false_values: Vec<String>,
        unknown_as: Option<bool>,
    ) -> Result<Self, InterpretationError> {
        if true_values.is_empty() {
            return Err(InterpretationError::EmptyTrueValues);
        }
        if false_values.is_empty() {
            return Err(InterpretationError::EmptyFalseValues);
        }
        let overlapping = true_values
            .iter()
            .find(|value| false_values.contains(value));
        if let Some(value) = overlapping {
            return Err(InterpretationError::OverlappingBoolValue {
                value: (*value).clone(),
            });
        }

        Ok(Self {
            true_values,
            false_values,
            unknown_as,
        })
    }

    /// Ordered wire values mapped to true.
    #[must_use]
    pub fn true_values(&self) -> &[String] {
        &self.true_values
    }

    /// Ordered wire values mapped to false.
    #[must_use]
    pub fn false_values(&self) -> &[String] {
        &self.false_values
    }

    /// Interpretation for values outside both lists.
    #[must_use]
    pub fn unknown_as(&self) -> Option<bool> {
        self.unknown_as
    }
}

/// Checked list of wire sentinel values treated as absence.
///
/// Construction requires only a nonempty list; empty strings and duplicates
/// are allowed and order is retained.
#[derive(Debug, Clone, PartialEq)]
pub struct SentinelValues(Vec<String>);

impl SentinelValues {
    /// Creates checked sentinel values.
    ///
    /// # Errors
    ///
    /// Returns `EmptySentinels` when the list is empty.
    pub fn new(values: Vec<String>) -> Result<Self, InterpretationError> {
        if values.is_empty() {
            return Err(InterpretationError::EmptySentinels);
        }

        Ok(Self(values))
    }

    /// Ordered sentinel values.
    #[must_use]
    pub fn values(&self) -> &[String] {
        &self.0
    }
}

/// Local construction invariant violated by an interpretation value.
///
/// These errors are returned by checked constructors before insertion. They do
/// not participate in graph finalization errors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InterpretationError {
    /// A coordinate field name was empty.
    #[error("coordinate field {index} is empty")]
    EmptyCoordinateField {
        /// Declared position of the empty field name.
        index: usize,
    },
    /// Coordinate field names were not distinct.
    #[error("coordinate fields must be distinct")]
    DuplicateCoordinateField,
    /// The coordinate delimiter was empty.
    #[error("coordinate delimiter must not be empty")]
    EmptyCoordinateDelimiter,
    /// The true-value mapping was empty.
    #[error("true-value mapping must not be empty")]
    EmptyTrueValues,
    /// The false-value mapping was empty.
    #[error("false-value mapping must not be empty")]
    EmptyFalseValues,
    /// A wire value appeared in both boolean mappings.
    #[error("boolean mappings overlap at {value:?}")]
    OverlappingBoolValue {
        /// First overlapping value in true-list order.
        value: String,
    },
    /// The sentinel list was empty.
    #[error("sentinel values must not be empty")]
    EmptySentinels,
}
