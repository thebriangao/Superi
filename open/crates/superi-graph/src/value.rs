//! Neutral typed values shared by graph-owning domains and processing catalogs.
//!
//! A graph retains one payload type across every node. [`GraphValue`] keeps the owning domain's
//! exact payload while adding finite processing values that can be edited, linked, serialized,
//! inspected, and evaluated without a dependency on any concrete node catalog.

use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::expr::ExpressionParameterValue;
use crate::node::ValueTypeId;

const COMPONENT: &str = "superi-graph.value";
const MAX_CHOICE_BYTES: usize = 256;

/// One finite binary64 value whose exact bits define equality and serialization.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FiniteF64(u64);

impl FiniteF64 {
    /// Validates and preserves one finite binary64 value.
    ///
    /// # Errors
    ///
    /// Returns invalid input for NaN or either infinity.
    pub fn new(value: f64) -> Result<Self> {
        if value.is_finite() {
            Ok(Self(value.to_bits()))
        } else {
            Err(value_error(
                "finite_number",
                "nonfinite_number",
                "graph numeric values must be finite",
            ))
        }
    }

    /// Validates one exact binary64 representation.
    ///
    /// # Errors
    ///
    /// Returns invalid input when the bits encode NaN or either infinity.
    pub fn from_bits(bits: u64) -> Result<Self> {
        Self::new(f64::from_bits(bits))
    }

    /// Returns the finite number.
    #[must_use]
    pub fn get(self) -> f64 {
        f64::from_bits(self.0)
    }

    /// Returns the exact retained binary64 representation.
    #[must_use]
    pub const fn to_bits(self) -> u64 {
        self.0
    }
}

impl Serialize for FiniteF64 {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.0)
    }
}

impl<'de> Deserialize<'de> for FiniteF64 {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bits = u64::deserialize(deserializer)?;
        Self::from_bits(bits).map_err(D::Error::custom)
    }
}

impl fmt::Display for FiniteF64 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

/// One bounded editable discrete choice.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChoiceValue(String);

impl ChoiceValue {
    /// Creates a nonempty bounded UTF-8 choice.
    ///
    /// # Errors
    ///
    /// Returns invalid input when the choice is empty or exceeds 256 UTF-8 bytes.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() {
            return Err(value_error(
                "choice",
                "empty_choice",
                "graph choice values cannot be empty",
            ));
        }
        if value.len() > MAX_CHOICE_BYTES {
            return Err(value_error(
                "choice",
                "choice_too_long",
                "graph choice values exceed the supported length",
            ));
        }
        Ok(Self(value))
    }

    /// Returns the authored choice text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for ChoiceValue {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ChoiceValue {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl fmt::Display for ChoiceValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One neutral graph payload that retains domain values and typed processing state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum GraphValue<T> {
    /// Exact state owned by the graph's domain, such as a timeline object.
    Domain(T),
    /// One finite numeric parameter.
    Scalar(FiniteF64),
    /// One finite two-component value.
    Vec2([FiniteF64; 2]),
    /// One finite three-component value.
    Vec3([FiniteF64; 3]),
    /// One finite RGBA value.
    Color([FiniteF64; 4]),
    /// One finite row-major 3 by 3 matrix.
    Matrix3([FiniteF64; 9]),
    /// One Boolean parameter.
    Boolean(bool),
    /// One bounded discrete choice.
    Choice(ChoiceValue),
}

impl<T> GraphValue<T> {
    /// Wraps exact graph-domain state without conversion.
    #[must_use]
    pub const fn domain(value: T) -> Self {
        Self::Domain(value)
    }

    /// Creates one finite scalar.
    ///
    /// # Errors
    ///
    /// Returns invalid input for a nonfinite value.
    pub fn scalar(value: f64) -> Result<Self> {
        Ok(Self::Scalar(FiniteF64::new(value)?))
    }

    /// Creates one finite two-component value.
    ///
    /// # Errors
    ///
    /// Returns invalid input when any component is nonfinite.
    pub fn vec2(value: [f64; 2]) -> Result<Self> {
        Ok(Self::Vec2(finite_array(value)?))
    }

    /// Creates one finite three-component value.
    ///
    /// # Errors
    ///
    /// Returns invalid input when any component is nonfinite.
    pub fn vec3(value: [f64; 3]) -> Result<Self> {
        Ok(Self::Vec3(finite_array(value)?))
    }

    /// Creates one finite RGBA value.
    ///
    /// # Errors
    ///
    /// Returns invalid input when any component is nonfinite.
    pub fn color(value: [f64; 4]) -> Result<Self> {
        Ok(Self::Color(finite_array(value)?))
    }

    /// Creates one finite row-major 3 by 3 matrix.
    ///
    /// # Errors
    ///
    /// Returns invalid input when any component is nonfinite.
    pub fn matrix3(value: [f64; 9]) -> Result<Self> {
        Ok(Self::Matrix3(finite_array(value)?))
    }

    /// Creates one Boolean value.
    #[must_use]
    pub const fn boolean(value: bool) -> Self {
        Self::Boolean(value)
    }

    /// Creates one bounded discrete choice.
    ///
    /// # Errors
    ///
    /// Returns invalid input for an empty or oversized choice.
    pub fn choice(value: impl Into<String>) -> Result<Self> {
        Ok(Self::Choice(ChoiceValue::new(value)?))
    }

    /// Borrows the domain payload when this is a domain value.
    #[must_use]
    pub const fn as_domain(&self) -> Option<&T> {
        match self {
            Self::Domain(value) => Some(value),
            _ => None,
        }
    }

    /// Returns the scalar when this is a scalar value.
    #[must_use]
    pub fn as_scalar(&self) -> Option<f64> {
        match self {
            Self::Scalar(value) => Some(value.get()),
            _ => None,
        }
    }

    /// Returns the two components when this is a two-component value.
    #[must_use]
    pub fn as_vec2(&self) -> Option<[f64; 2]> {
        match self {
            Self::Vec2(value) => Some(unpack_array(value)),
            _ => None,
        }
    }

    /// Returns the three components when this is a three-component value.
    #[must_use]
    pub fn as_vec3(&self) -> Option<[f64; 3]> {
        match self {
            Self::Vec3(value) => Some(unpack_array(value)),
            _ => None,
        }
    }

    /// Returns the four RGBA components when this is a color value.
    #[must_use]
    pub fn as_color(&self) -> Option<[f64; 4]> {
        match self {
            Self::Color(value) => Some(unpack_array(value)),
            _ => None,
        }
    }

    /// Returns the row-major 3 by 3 matrix when this is a matrix value.
    #[must_use]
    pub fn as_matrix3(&self) -> Option<[f64; 9]> {
        match self {
            Self::Matrix3(value) => Some(unpack_array(value)),
            _ => None,
        }
    }

    /// Returns the Boolean when this is a Boolean value.
    #[must_use]
    pub const fn as_boolean(&self) -> Option<bool> {
        match self {
            Self::Boolean(value) => Some(*value),
            _ => None,
        }
    }

    /// Borrows the authored choice when this is a choice value.
    #[must_use]
    pub fn as_choice(&self) -> Option<&str> {
        match self {
            Self::Choice(value) => Some(value.as_str()),
            _ => None,
        }
    }

    /// Returns the stable payload variant code used by diagnostics and fingerprints.
    #[must_use]
    pub const fn kind_code(&self) -> &'static str {
        match self {
            Self::Domain(_) => "domain",
            Self::Scalar(_) => "scalar",
            Self::Vec2(_) => "vec2",
            Self::Vec3(_) => "vec3",
            Self::Color(_) => "color",
            Self::Matrix3(_) => "matrix3",
            Self::Boolean(_) => "boolean",
            Self::Choice(_) => "choice",
        }
    }
}

impl<T: Clone> ExpressionParameterValue for GraphValue<T> {
    fn to_expression_scalar(&self, value_type: &ValueTypeId) -> Result<f64> {
        self.as_scalar().ok_or_else(|| {
            value_error(
                "expression_input",
                "non_scalar_expression_value",
                "only scalar graph values can drive numeric expressions",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "expression_input")
                    .with_field("value_type", value_type.as_str())
                    .with_field("value_kind", self.kind_code()),
            )
        })
    }

    fn from_expression_scalar(value_type: &ValueTypeId, value: f64) -> Result<Self> {
        Self::scalar(value).map_err(|mut error| {
            error.push_context(
                ErrorContext::new(COMPONENT, "expression_result")
                    .with_field("value_type", value_type.as_str()),
            );
            error
        })
    }
}

fn finite_array<const N: usize>(value: [f64; N]) -> Result<[FiniteF64; N]> {
    let mut output = [FiniteF64(0); N];
    for (destination, source) in output.iter_mut().zip(value) {
        *destination = FiniteF64::new(source)?;
    }
    Ok(output)
}

fn unpack_array<const N: usize>(value: &[FiniteF64; N]) -> [f64; N] {
    let mut output = [0.0; N];
    for (destination, source) in output.iter_mut().zip(value) {
        *destination = source.get();
    }
    output
}

fn value_error(operation: &'static str, reason: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation).with_field("reason", reason))
}
