//! Typed parameter links and bounded pure expressions.
//!
//! Drivers are ordinary inspectable graph state. Expressions retain their editable source and a
//! checked postfix program whose only inputs are explicit typed parameter references. The language
//! deliberately has no I/O, mutation, loops, recursion, functions, or host script escape.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::ids::{NodeId, ParameterId};
use crate::node::ValueTypeId;

const COMPONENT: &str = "superi-graph.expression";
const MAX_SOURCE_BYTES: usize = 4096;
const MAX_INSTRUCTIONS: usize = 512;
const MAX_DEPTH: usize = 64;
const MAX_VARIABLE_BYTES: usize = 64;

/// One globally addressable parameter inside a graph snapshot.
///
/// Parameter identities are node-local, so every dependency endpoint includes both domains.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ParameterAddress {
    node_id: NodeId,
    parameter_id: ParameterId,
}

impl ParameterAddress {
    /// Creates one graph-local parameter address.
    #[must_use]
    pub const fn new(node_id: NodeId, parameter_id: ParameterId) -> Self {
        Self {
            node_id,
            parameter_id,
        }
    }

    /// Returns the node that owns the parameter.
    #[must_use]
    pub const fn node_id(self) -> NodeId {
        self.node_id
    }

    /// Returns the stable parameter identity within the node.
    #[must_use]
    pub const fn parameter_id(self) -> ParameterId {
        self.parameter_id
    }
}

impl fmt::Display for ParameterAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.node_id, self.parameter_id)
    }
}

/// One explicit typed dependency on another editable parameter.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ParameterReference {
    address: ParameterAddress,
    value_type: ValueTypeId,
}

impl ParameterReference {
    /// Creates a dependency with the exact type expected by the driver author.
    #[must_use]
    pub const fn new(address: ParameterAddress, value_type: ValueTypeId) -> Self {
        Self {
            address,
            value_type,
        }
    }

    /// Returns the referenced parameter address.
    #[must_use]
    pub const fn address(&self) -> ParameterAddress {
        self.address
    }

    /// Returns the exact expected source type.
    #[must_use]
    pub const fn value_type(&self) -> &ValueTypeId {
        &self.value_type
    }
}

/// A stable source-local variable name used by one expression.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ExpressionVariableName(String);

impl ExpressionVariableName {
    /// Creates a bounded ASCII identifier.
    ///
    /// # Errors
    ///
    /// Returns invalid input when the name is empty, too long, or not an ASCII identifier.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let bytes = value.as_bytes();
        if bytes.is_empty() {
            return Err(expression_error(
                "variable_name",
                "empty_variable_name",
                "expression variable name cannot be empty",
            ));
        }
        if bytes.len() > MAX_VARIABLE_BYTES {
            return Err(expression_error(
                "variable_name",
                "variable_name_too_long",
                "expression variable name exceeds the supported length",
            ));
        }
        if !is_identifier_start(bytes[0])
            || bytes[1..].iter().any(|byte| !is_identifier_continue(*byte))
        {
            return Err(expression_error(
                "variable_name",
                "invalid_variable_name",
                "expression variable name must be an ASCII identifier",
            ));
        }
        Ok(Self(value))
    }

    /// Returns the canonical variable spelling.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ExpressionVariableName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ExpressionVariableName {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::new(value)
    }
}

/// One checked postfix expression operation.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ExpressionInstruction {
    /// Pushes one finite decimal literal, preserved exactly as authored.
    Constant(String),
    /// Pushes one explicitly bound parameter variable.
    Variable(ExpressionVariableName),
    /// Negates the top value.
    Negate,
    /// Adds the top two values.
    Add,
    /// Subtracts the top value from the value below it.
    Subtract,
    /// Multiplies the top two values.
    Multiply,
    /// Divides the value below the top by the top value.
    Divide,
}

/// Editable source plus its checked deterministic expression program.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParameterExpression {
    source: String,
    variables: BTreeMap<ExpressionVariableName, ParameterReference>,
    instructions: Vec<ExpressionInstruction>,
}

impl ParameterExpression {
    /// Compiles a bounded pure arithmetic expression with explicit typed variables.
    ///
    /// The language supports finite decimal constants, variables, parentheses, unary negation,
    /// addition, subtraction, multiplication, and division. Every referenced variable must have
    /// exactly one binding, and unused bindings are rejected so dependency inspection is complete.
    ///
    /// # Errors
    ///
    /// Returns user-correctable invalid input for syntax, bounds, duplicate bindings, missing
    /// bindings, unused bindings, or nonfinite constants.
    pub fn compile(
        source: &str,
        variables: impl IntoIterator<Item = (ExpressionVariableName, ParameterReference)>,
    ) -> Result<Self> {
        let source = source.trim().to_owned();
        if source.is_empty() {
            return Err(expression_error(
                "compile",
                "empty_expression",
                "parameter expression cannot be empty",
            ));
        }
        if source.len() > MAX_SOURCE_BYTES {
            return Err(expression_error(
                "compile",
                "expression_source_too_long",
                "parameter expression exceeds the supported source length",
            ));
        }

        let mut bindings = BTreeMap::new();
        for (name, reference) in variables {
            if bindings.insert(name.clone(), reference).is_some() {
                return Err(expression_error(
                    "compile",
                    "duplicate_variable_binding",
                    "parameter expression variable is bound more than once",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "compile").with_field("variable", name.as_str()),
                ));
            }
        }

        let mut parser = Parser::new(&source, &bindings);
        parser.parse_expression(0)?;
        parser.skip_whitespace();
        if parser.position != source.len() {
            return Err(parser.syntax_error(
                "unexpected_token",
                "parameter expression contains an unexpected token",
            ));
        }
        if let Some(unused) = bindings
            .keys()
            .find(|name| !parser.referenced.contains(*name))
        {
            return Err(expression_error(
                "compile",
                "unused_variable_binding",
                "parameter expression binding is not used by the source",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "compile").with_field("variable", unused.as_str()),
            ));
        }
        let instructions = std::mem::take(&mut parser.instructions);
        drop(parser);

        Ok(Self {
            source,
            variables: bindings,
            instructions,
        })
    }

    /// Returns the editable source exactly after outer whitespace trimming.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns explicit variables in canonical name order.
    #[must_use]
    pub const fn variables(&self) -> &BTreeMap<ExpressionVariableName, ParameterReference> {
        &self.variables
    }

    /// Returns the checked postfix program in execution order.
    #[must_use]
    pub fn instructions(&self) -> &[ExpressionInstruction] {
        &self.instructions
    }

    /// Evaluates the checked program through its explicit variable bindings.
    ///
    /// # Errors
    ///
    /// Returns the resolver's error, or user-correctable invalid input for nonfinite inputs,
    /// division by zero, or a nonfinite result.
    pub fn evaluate_with(
        &self,
        mut resolve: impl FnMut(&ExpressionVariableName, &ParameterReference) -> Result<f64>,
    ) -> Result<f64> {
        let mut stack = Vec::with_capacity(self.instructions.len());
        for instruction in &self.instructions {
            match instruction {
                ExpressionInstruction::Constant(value) => {
                    let value = value
                        .parse::<f64>()
                        .expect("compiled expression constant remains valid");
                    stack.push(value);
                }
                ExpressionInstruction::Variable(name) => {
                    let reference = self
                        .variables
                        .get(name)
                        .expect("compiled expression variable remains bound");
                    let value = resolve(name, reference)?;
                    if !value.is_finite() {
                        return Err(expression_error(
                            "evaluate",
                            "nonfinite_variable_value",
                            "expression variable resolved to a nonfinite value",
                        )
                        .with_context(
                            ErrorContext::new(COMPONENT, "evaluate")
                                .with_field("variable", name.as_str())
                                .with_field("parameter", reference.address().to_string()),
                        ));
                    }
                    stack.push(value);
                }
                ExpressionInstruction::Negate => {
                    let value = pop_unary(&mut stack)?;
                    stack.push(-value);
                }
                ExpressionInstruction::Add => {
                    let (left, right) = pop_binary(&mut stack)?;
                    stack.push(left + right);
                }
                ExpressionInstruction::Subtract => {
                    let (left, right) = pop_binary(&mut stack)?;
                    stack.push(left - right);
                }
                ExpressionInstruction::Multiply => {
                    let (left, right) = pop_binary(&mut stack)?;
                    stack.push(left * right);
                }
                ExpressionInstruction::Divide => {
                    let (left, right) = pop_binary(&mut stack)?;
                    if right == 0.0 {
                        return Err(expression_error(
                            "evaluate",
                            "division_by_zero",
                            "parameter expression divides by zero",
                        ));
                    }
                    stack.push(left / right);
                }
            }
            if stack.last().is_some_and(|value| !value.is_finite()) {
                return Err(expression_error(
                    "evaluate",
                    "nonfinite_expression_result",
                    "parameter expression produced a nonfinite value",
                ));
            }
        }
        if stack.len() != 1 {
            return Err(invalid_program_error());
        }
        Ok(stack[0])
    }
}

/// One editable driver attached to a target parameter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParameterDriver {
    value_type: ValueTypeId,
    kind: ParameterDriverKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ParameterDriverKind {
    Link(ParameterReference),
    Expression(ParameterExpression),
}

impl ParameterDriver {
    /// Creates a lossless direct parameter link.
    #[must_use]
    pub const fn link(value_type: ValueTypeId, source: ParameterReference) -> Self {
        Self {
            value_type,
            kind: ParameterDriverKind::Link(source),
        }
    }

    /// Creates a numeric expression driver.
    #[must_use]
    pub const fn expression(value_type: ValueTypeId, expression: ParameterExpression) -> Self {
        Self {
            value_type,
            kind: ParameterDriverKind::Expression(expression),
        }
    }

    /// Returns the exact target result type declared by the driver.
    #[must_use]
    pub const fn value_type(&self) -> &ValueTypeId {
        &self.value_type
    }

    /// Returns the direct source when this is a link.
    #[must_use]
    pub const fn as_link(&self) -> Option<&ParameterReference> {
        match &self.kind {
            ParameterDriverKind::Link(source) => Some(source),
            ParameterDriverKind::Expression(_) => None,
        }
    }

    /// Returns the expression when this is an expression driver.
    #[must_use]
    pub const fn as_expression(&self) -> Option<&ParameterExpression> {
        match &self.kind {
            ParameterDriverKind::Link(_) => None,
            ParameterDriverKind::Expression(expression) => Some(expression),
        }
    }

    /// Returns unique dependency references in canonical parameter-address order.
    #[must_use]
    pub fn dependencies(&self) -> Vec<&ParameterReference> {
        match &self.kind {
            ParameterDriverKind::Link(source) => vec![source],
            ParameterDriverKind::Expression(expression) => {
                let mut dependencies = expression.variables().values().collect::<Vec<_>>();
                dependencies.sort();
                dependencies.dedup();
                dependencies
            }
        }
    }
}

/// Conversion between a domain-owned parameter payload and the expression scalar domain.
///
/// Direct links never call this trait and always clone the exact typed payload. A concrete catalog
/// implements the conversion only for value types that permit numeric expressions.
pub trait ExpressionParameterValue: Clone {
    /// Converts one typed payload into a finite scalar expression value.
    fn to_expression_scalar(&self, value_type: &ValueTypeId) -> Result<f64>;

    /// Constructs one typed payload from a finite scalar expression result.
    fn from_expression_scalar(value_type: &ValueTypeId, value: f64) -> Result<Self>;
}

struct Parser<'a> {
    source: &'a str,
    bindings: &'a BTreeMap<ExpressionVariableName, ParameterReference>,
    position: usize,
    instructions: Vec<ExpressionInstruction>,
    referenced: BTreeSet<ExpressionVariableName>,
}

impl<'a> Parser<'a> {
    fn new(
        source: &'a str,
        bindings: &'a BTreeMap<ExpressionVariableName, ParameterReference>,
    ) -> Self {
        Self {
            source,
            bindings,
            position: 0,
            instructions: Vec::new(),
            referenced: BTreeSet::new(),
        }
    }

    fn parse_expression(&mut self, depth: usize) -> Result<()> {
        self.parse_term(depth)?;
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b'+') => {
                    self.position += 1;
                    self.parse_term(depth)?;
                    self.push(ExpressionInstruction::Add)?;
                }
                Some(b'-') => {
                    self.position += 1;
                    self.parse_term(depth)?;
                    self.push(ExpressionInstruction::Subtract)?;
                }
                _ => return Ok(()),
            }
        }
    }

    fn parse_term(&mut self, depth: usize) -> Result<()> {
        self.parse_unary(depth)?;
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b'*') => {
                    self.position += 1;
                    self.parse_unary(depth)?;
                    self.push(ExpressionInstruction::Multiply)?;
                }
                Some(b'/') => {
                    self.position += 1;
                    self.parse_unary(depth)?;
                    self.push(ExpressionInstruction::Divide)?;
                }
                _ => return Ok(()),
            }
        }
    }

    fn parse_unary(&mut self, depth: usize) -> Result<()> {
        self.skip_whitespace();
        if self.peek() == Some(b'-') {
            self.check_depth(depth)?;
            self.position += 1;
            self.parse_unary(depth + 1)?;
            self.push(ExpressionInstruction::Negate)
        } else {
            self.parse_primary(depth)
        }
    }

    fn parse_primary(&mut self, depth: usize) -> Result<()> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'(') => {
                self.check_depth(depth)?;
                self.position += 1;
                self.parse_expression(depth + 1)?;
                self.skip_whitespace();
                if self.peek() != Some(b')') {
                    return Err(self.syntax_error(
                        "missing_closing_parenthesis",
                        "parameter expression is missing a closing parenthesis",
                    ));
                }
                self.position += 1;
                Ok(())
            }
            Some(byte) if byte.is_ascii_digit() || byte == b'.' => self.parse_number(),
            Some(byte) if is_identifier_start(byte) => self.parse_variable(),
            Some(_) => {
                Err(self.syntax_error("expected_value", "parameter expression expected a value"))
            }
            None => Err(self.syntax_error(
                "unexpected_end",
                "parameter expression ended before a value",
            )),
        }
    }

    fn parse_number(&mut self) -> Result<()> {
        let start = self.position;
        let mut digits = 0;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.position += 1;
            digits += 1;
        }
        if self.peek() == Some(b'.') {
            self.position += 1;
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.position += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            return Err(self.syntax_error(
                "invalid_number",
                "parameter expression contains an invalid number",
            ));
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.position += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.position += 1;
            }
            let exponent_start = self.position;
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.position += 1;
            }
            if exponent_start == self.position {
                return Err(self.syntax_error(
                    "invalid_number",
                    "parameter expression contains an invalid number exponent",
                ));
            }
        }
        let literal = self.source[start..self.position].to_owned();
        let value = literal.parse::<f64>().map_err(|_| {
            self.syntax_error(
                "invalid_number",
                "parameter expression contains an invalid number",
            )
        })?;
        if !value.is_finite() {
            return Err(self.syntax_error(
                "nonfinite_constant",
                "parameter expression constant must be finite",
            ));
        }
        self.push(ExpressionInstruction::Constant(literal))
    }

    fn parse_variable(&mut self) -> Result<()> {
        let start = self.position;
        self.position += 1;
        while self.peek().is_some_and(is_identifier_continue) {
            self.position += 1;
        }
        let spelling = self.source[start..self.position].to_owned();
        let name = ExpressionVariableName::new(spelling)?;
        if !self.bindings.contains_key(&name) {
            return Err(expression_error(
                "compile",
                "missing_variable_binding",
                "parameter expression variable has no explicit binding",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "compile")
                    .with_field("variable", name.as_str())
                    .with_field("offset", start.to_string()),
            ));
        }
        self.referenced.insert(name.clone());
        self.push(ExpressionInstruction::Variable(name))
    }

    fn push(&mut self, instruction: ExpressionInstruction) -> Result<()> {
        if self.instructions.len() >= MAX_INSTRUCTIONS {
            return Err(self.syntax_error(
                "expression_instruction_limit",
                "parameter expression exceeds the supported instruction count",
            ));
        }
        self.instructions.push(instruction);
        Ok(())
    }

    fn check_depth(&self, depth: usize) -> Result<()> {
        if depth >= MAX_DEPTH {
            Err(self.syntax_error(
                "expression_depth_exceeded",
                "parameter expression exceeds the supported nesting depth",
            ))
        } else {
            Ok(())
        }
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.position += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.source.as_bytes().get(self.position).copied()
    }

    fn syntax_error(&self, reason: &'static str, message: &'static str) -> Error {
        expression_error("compile", reason, message).with_context(
            ErrorContext::new(COMPONENT, "compile")
                .with_field("offset", self.position.to_string())
                .with_field("source_bytes", self.source.len().to_string()),
        )
    }
}

fn pop_unary(stack: &mut Vec<f64>) -> Result<f64> {
    stack.pop().ok_or_else(invalid_program_error)
}

fn pop_binary(stack: &mut Vec<f64>) -> Result<(f64, f64)> {
    let right = stack.pop().ok_or_else(invalid_program_error)?;
    let left = stack.pop().ok_or_else(invalid_program_error)?;
    Ok((left, right))
}

fn invalid_program_error() -> Error {
    Error::new(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "checked parameter expression program is invalid",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "evaluate").with_field("reason", "invalid_checked_program"),
    )
}

fn expression_error(operation: &'static str, reason: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation).with_field("reason", reason))
}

const fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

const fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
