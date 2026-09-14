use la_arena::Idx;
use serde_json::{Number, Value};

use crate::SourceRef;

/// Opaque identity of a definition within one graph.
///
/// Identity is based on allocation, not the definition's source name or shape.
/// IDs must be used only with the builder or API mapping that issued them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefinitionId(Idx<Definition>);

impl DefinitionId {
    pub(crate) fn from_definition_index(index: Idx<Definition>) -> Self {
        Self(index)
    }

    pub(crate) fn from_slot_index(index: Idx<Option<Definition>>) -> Self {
        Self(Idx::from_raw(index.into_raw()))
    }

    pub(crate) fn definition_index(self) -> Idx<Definition> {
        self.0
    }

    pub(crate) fn slot_index(self) -> Idx<Option<Definition>> {
        Idx::from_raw(self.0.into_raw())
    }

    pub(crate) fn raw_index(self) -> usize {
        self.0.into_raw().into_u32() as usize
    }
}

/// A named source definition and its root schema use.
#[derive(Debug, Clone, PartialEq)]
pub struct Definition {
    /// Name supplied by the frontend before target-specific naming.
    pub source_name: String,
    /// Root use of the definition's schema.
    pub schema: SchemaUse,
}

/// One occurrence of a schema expression and its local semantics.
///
/// `nullable` augments the base expression with null. A false value does not
/// remove nullability admitted by a referenced definition or by
/// [`TypeExpr::AnyJson`].
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaUse {
    /// Inline expression or reference used at this location.
    pub ty: TypeExpr,
    /// Whether this use additionally accepts null.
    pub nullable: bool,
    /// Annotations local to this use.
    pub annotations: SchemaAnnotations,
}

impl SchemaUse {
    /// Creates a non-null-augmented schema use without annotations.
    #[must_use]
    pub fn new(ty: TypeExpr) -> Self {
        Self {
            ty,
            nullable: false,
            annotations: SchemaAnnotations::default(),
        }
    }
}

/// Annotations attached to one schema use.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SchemaAnnotations {
    /// Human-readable schema description.
    pub description: Option<String>,
    /// Declared semantic format.
    pub format: Option<String>,
    /// Declared JSON default; JSON null is distinct from no default.
    pub default: Option<Value>,
    /// Provenance of this use.
    pub source: Option<SourceRef>,
}

/// Target-neutral schema expression.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    /// A string schema.
    String(StringSchema),
    /// An integer schema.
    Integer(IntegerSchema),
    /// A number schema.
    Number(NumberSchema),
    /// A boolean schema.
    Boolean,
    /// An array schema with an owned item use.
    Array(ArraySchema),
    /// An object schema with ordered properties.
    Object(ObjectSchema),
    /// An unconstrained JSON value.
    AnyJson,
    /// A reference to a separately allocated definition.
    Ref(DefinitionId),
}

/// String-specific constraints.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StringSchema {
    /// Length and pattern constraints.
    pub constraints: StringConstraints,
    /// Ordered enum values; an empty vector remains distinct from no enum.
    pub enum_values: Option<Vec<String>>,
    /// A separately declared constant string value.
    pub const_value: Option<String>,
}

/// Integer-specific constraints.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IntegerSchema {
    /// Numeric bounds retained without target-width selection.
    pub constraints: NumericConstraints,
}

/// Number-specific constraints.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NumberSchema {
    /// Numeric bounds retained without floating-point conversion.
    pub constraints: NumericConstraints,
}

/// Constraints on a string value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StringConstraints {
    /// Inclusive minimum string length.
    pub min_length: Option<u64>,
    /// Inclusive maximum string length.
    pub max_length: Option<u64>,
    /// Declared pattern text; graph finalization does not compile it.
    pub pattern: Option<String>,
}

/// Bounds on an integer or number value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NumericConstraints {
    /// Lower bound, when declared.
    pub minimum: Option<NumericBound>,
    /// Upper bound, when declared.
    pub maximum: Option<NumericBound>,
}

/// One numeric bound and whether it excludes its value.
#[derive(Debug, Clone, PartialEq)]
pub struct NumericBound {
    /// Numeric bound retained as parsed JSON number data.
    pub value: Number,
    /// Whether the bound excludes `value`.
    pub exclusive: bool,
}

/// Array item schema and constraints.
#[derive(Debug, Clone, PartialEq)]
pub struct ArraySchema {
    /// Owned use of the item schema.
    pub items: Box<SchemaUse>,
    /// Array length constraints.
    pub constraints: ArrayConstraints,
}

/// Constraints on an array value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArrayConstraints {
    /// Inclusive minimum item count.
    pub min_items: Option<u64>,
    /// Inclusive maximum item count.
    pub max_items: Option<u64>,
}

/// Object properties and its unknown-property rule.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectSchema {
    /// Properties in source order.
    pub properties: Vec<Property>,
    /// Rule for properties not listed in `properties`.
    pub additional_properties: AdditionalProperties,
}

/// One ordered object property.
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    /// Property name on the wire.
    pub wire_name: String,
    /// Whether the property must be present.
    pub required: bool,
    /// Schema use for the property's value.
    pub value: SchemaUse,
}

/// Rule for object properties not explicitly listed.
#[derive(Debug, Clone, PartialEq)]
pub enum AdditionalProperties {
    /// No additional-properties rule was declared.
    Unspecified,
    /// Arbitrary additional JSON values are allowed.
    Allowed,
    /// Additional properties are forbidden.
    Forbidden,
    /// Additional values must match the owned schema use.
    Schema(Box<SchemaUse>),
}
