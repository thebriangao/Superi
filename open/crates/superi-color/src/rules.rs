//! Deterministic display, view, look, and deliverable output rules.
//!
//! Rules select an explicit pipeline from retained source semantics. They do
//! not reinterpret source pixels, mutate graded working images, discover a
//! monitor, or perform presentation. A selected pipeline applies named looks
//! in declared order and then delegates encoding to [`OutputColorTransform`].

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_image::value::Image;

use crate::lut::{DomainPolicy, Lut, LutInterpolation};
use crate::transform_in::InputSourceKind;
use crate::transform_out::{OutputColorTransform, OutputTargetKind};
use crate::working_space::{WorkingImageF32, WorkingSpace};

const COMPONENT: &str = "superi-color.rules";
const MAX_NAME_BYTES: usize = 128;

/// Source semantics retained after explicit interpretation into working space.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum SourceRole {
    /// Scene-referred source values that may require an output-rendering view.
    SceneReferred,
    /// Display-referred source values that must avoid an unintended second rendering view.
    DisplayReferred,
}

impl SourceRole {
    /// Returns the stable diagnostic code for this source role.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::SceneReferred => "scene-referred",
            Self::DisplayReferred => "display-referred",
        }
    }
}

impl From<InputSourceKind> for SourceRole {
    fn from(source: InputSourceKind) -> Self {
        match source {
            InputSourceKind::Camera | InputSourceKind::SceneReferred => Self::SceneReferred,
            InputSourceKind::DisplayReferred => Self::DisplayReferred,
        }
    }
}

/// Explicit source-role filter for one view or output rule.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ViewApplicability {
    /// The rule is valid for every explicitly classified source.
    Any,
    /// The rule is valid only for the selected source role.
    Only(SourceRole),
}

impl ViewApplicability {
    /// Returns whether this rule accepts a source role.
    #[must_use]
    pub const fn accepts(self, source: SourceRole) -> bool {
        match (self, source) {
            (Self::Any, _) => true,
            (Self::Only(SourceRole::SceneReferred), SourceRole::SceneReferred)
            | (Self::Only(SourceRole::DisplayReferred), SourceRole::DisplayReferred) => true,
            (Self::Only(_), _) => false,
        }
    }
}

/// One named creative transform applied in an explicit working process space.
#[derive(Clone, Debug, PartialEq)]
pub struct LookRule {
    name: String,
    process_space: WorkingSpace,
    lut: Lut,
    interpolation: LutInterpolation,
    domain_policy: DomainPolicy,
}

impl LookRule {
    /// Creates a named look with explicit LUT application policy.
    pub fn new(
        name: impl Into<String>,
        process_space: WorkingSpace,
        lut: Lut,
        interpolation: LutInterpolation,
        domain_policy: DomainPolicy,
    ) -> Result<Self> {
        let name = valid_name(name.into(), "create_look_rule")?;
        Ok(Self {
            name,
            process_space,
            lut,
            interpolation,
            domain_policy,
        })
    }

    /// Returns the stable look name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the required scene-linear process space.
    #[must_use]
    pub const fn process_space(&self) -> WorkingSpace {
        self.process_space
    }

    /// Returns the creative lookup table.
    #[must_use]
    pub const fn lut(&self) -> &Lut {
        &self.lut
    }

    /// Returns the requested lookup interpolation.
    #[must_use]
    pub const fn interpolation(&self) -> LutInterpolation {
        self.interpolation
    }

    /// Returns the explicit out-of-domain behavior.
    #[must_use]
    pub const fn domain_policy(&self) -> DomainPolicy {
        self.domain_policy
    }

    fn apply(&self, image: &WorkingImageF32) -> Result<WorkingImageF32> {
        if image.space() != self.process_space {
            return Err(invalid(
                "apply_look_rule",
                "look process space does not match the working image",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "inspect_look_process_space")
                    .with_field("look", self.name.clone())
                    .with_field(
                        "expected_primaries",
                        self.process_space.color_space().primaries().code(),
                    )
                    .with_field(
                        "actual_primaries",
                        image.space().color_space().primaries().code(),
                    ),
            ));
        }
        self.lut
            .apply_to_working_image(image, self.interpolation, self.domain_policy)
            .map_err(|error| {
                error.with_context(
                    ErrorContext::new(COMPONENT, "apply_look_rule")
                        .with_field("look", self.name.clone()),
                )
            })
    }
}

/// One selectable monitoring view within an ordered display rule.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewRule {
    name: String,
    applicability: ViewApplicability,
    looks: Vec<String>,
    output: OutputColorTransform,
}

impl ViewRule {
    /// Creates a display view that applies ordered named looks before encoding.
    pub fn new(
        name: impl Into<String>,
        applicability: ViewApplicability,
        looks: Vec<String>,
        output: OutputColorTransform,
    ) -> Result<Self> {
        let name = valid_name(name.into(), "create_view_rule")?;
        validate_references(&looks, "create_view_rule")?;
        if output.target_kind() != OutputTargetKind::Display {
            return Err(invalid(
                "create_view_rule",
                "a display view requires a display output transform",
            )
            .with_context(rule_context("view", &name)));
        }
        Ok(Self {
            name,
            applicability,
            looks,
            output,
        })
    }

    /// Returns the stable view name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the source-role filter.
    #[must_use]
    pub const fn applicability(&self) -> ViewApplicability {
        self.applicability
    }

    /// Returns ordered look names.
    #[must_use]
    pub fn looks(&self) -> &[String] {
        &self.looks
    }

    /// Returns the final display encoder.
    #[must_use]
    pub const fn output(&self) -> OutputColorTransform {
        self.output
    }
}

/// An ordered set of selectable views for one display target.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayRule {
    name: String,
    views: Vec<ViewRule>,
}

impl DisplayRule {
    /// Creates a display with at least one uniquely named ordered view.
    pub fn new(name: impl Into<String>, views: Vec<ViewRule>) -> Result<Self> {
        let name = valid_name(name.into(), "create_display_rule")?;
        if views.is_empty() {
            return Err(invalid(
                "create_display_rule",
                "a display rule requires at least one view",
            )
            .with_context(rule_context("display", &name)));
        }
        validate_unique(views.iter().map(ViewRule::name), "view", &name)?;
        Ok(Self { name, views })
    }

    /// Returns the stable display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns views in default-selection order.
    #[must_use]
    pub fn views(&self) -> &[ViewRule] {
        &self.views
    }
}

/// One named final-delivery rule with an explicit output encoder.
#[derive(Clone, Debug, PartialEq)]
pub struct OutputRule {
    name: String,
    applicability: ViewApplicability,
    looks: Vec<String>,
    output: OutputColorTransform,
}

impl OutputRule {
    /// Creates a deliverable rule that applies ordered looks before encoding.
    pub fn new(
        name: impl Into<String>,
        applicability: ViewApplicability,
        looks: Vec<String>,
        output: OutputColorTransform,
    ) -> Result<Self> {
        let name = valid_name(name.into(), "create_output_rule")?;
        validate_references(&looks, "create_output_rule")?;
        if output.target_kind() != OutputTargetKind::Deliverable {
            return Err(invalid(
                "create_output_rule",
                "an output rule requires a deliverable output transform",
            )
            .with_context(rule_context("output", &name)));
        }
        Ok(Self {
            name,
            applicability,
            looks,
            output,
        })
    }

    /// Returns the stable output name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the source-role filter.
    #[must_use]
    pub const fn applicability(&self) -> ViewApplicability {
        self.applicability
    }

    /// Returns ordered look names.
    #[must_use]
    pub fn looks(&self) -> &[String] {
        &self.looks
    }

    /// Returns the final deliverable encoder.
    #[must_use]
    pub const fn output(&self) -> OutputColorTransform {
        self.output
    }
}

/// A validated immutable set of display, view, look, and output rules.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorRuleSet {
    looks: Vec<LookRule>,
    displays: Vec<DisplayRule>,
    outputs: Vec<OutputRule>,
}

impl ColorRuleSet {
    /// Validates all names, namespaces, and look references.
    pub fn new(
        looks: Vec<LookRule>,
        displays: Vec<DisplayRule>,
        outputs: Vec<OutputRule>,
    ) -> Result<Self> {
        validate_unique(looks.iter().map(LookRule::name), "look", "rule-set")?;
        validate_unique(
            displays.iter().map(DisplayRule::name),
            "display",
            "rule-set",
        )?;
        validate_unique(outputs.iter().map(OutputRule::name), "output", "rule-set")?;

        for display in &displays {
            for view in &display.views {
                validate_registered_looks(&looks, &view.looks, "view", view.name())?;
            }
        }
        for output in &outputs {
            validate_registered_looks(&looks, &output.looks, "output", output.name())?;
        }

        Ok(Self {
            looks,
            displays,
            outputs,
        })
    }

    /// Returns looks in declaration order.
    #[must_use]
    pub fn looks(&self) -> &[LookRule] {
        &self.looks
    }

    /// Returns displays in declaration order.
    #[must_use]
    pub fn displays(&self) -> &[DisplayRule] {
        &self.displays
    }

    /// Returns outputs in declaration order.
    #[must_use]
    pub fn outputs(&self) -> &[OutputRule] {
        &self.outputs
    }

    /// Resolves an explicit view or the first source-appropriate default view.
    pub fn select_view(
        &self,
        display: &str,
        requested_view: Option<&str>,
        source: SourceRole,
    ) -> Result<&ViewRule> {
        let display = self
            .displays
            .iter()
            .find(|candidate| candidate.name == display)
            .ok_or_else(|| missing("select_display_view", "display", display))?;

        match requested_view {
            Some(requested) => {
                let view = display
                    .views
                    .iter()
                    .find(|candidate| candidate.name == requested)
                    .ok_or_else(|| missing("select_display_view", "view", requested))?;
                if !view.applicability.accepts(source) {
                    return Err(invalid(
                        "select_display_view",
                        "requested view is not applicable to the source role",
                    )
                    .with_context(
                        rule_context("view", requested)
                            .with_field("display", display.name.clone())
                            .with_field("source_role", source.code()),
                    ));
                }
                Ok(view)
            }
            None => display
                .views
                .iter()
                .find(|view| view.applicability.accepts(source))
                .ok_or_else(|| {
                    invalid(
                        "select_display_view",
                        "display has no view applicable to the source role",
                    )
                    .with_context(
                        rule_context("display", &display.name)
                            .with_field("source_role", source.code()),
                    )
                }),
        }
    }

    /// Applies the selected view pipeline and produces a display-encoded image.
    pub fn render_display(
        &self,
        display: &str,
        requested_view: Option<&str>,
        source_role: SourceRole,
        image: &WorkingImageF32,
    ) -> Result<Image> {
        let view = self.select_view(display, requested_view, source_role)?;
        let rendered = self.apply_looks(&view.looks, image)?;
        view.output.apply_f32(&rendered).map_err(|error| {
            error.with_context(
                rule_context("view", view.name())
                    .with_field("display", display.to_owned())
                    .with_field("source_role", source_role.code()),
            )
        })
    }

    /// Applies a named final-delivery pipeline independently of display state.
    pub fn render_output(
        &self,
        output: &str,
        source_role: SourceRole,
        image: &WorkingImageF32,
    ) -> Result<Image> {
        let output = self
            .outputs
            .iter()
            .find(|candidate| candidate.name == output)
            .ok_or_else(|| missing("select_output_rule", "output", output))?;
        if !output.applicability.accepts(source_role) {
            return Err(invalid(
                "select_output_rule",
                "output rule is not applicable to the source role",
            )
            .with_context(
                rule_context("output", output.name()).with_field("source_role", source_role.code()),
            ));
        }
        let rendered = self.apply_looks(&output.looks, image)?;
        output.output.apply_f32(&rendered).map_err(|error| {
            error.with_context(
                rule_context("output", output.name()).with_field("source_role", source_role.code()),
            )
        })
    }

    fn apply_looks(&self, names: &[String], image: &WorkingImageF32) -> Result<WorkingImageF32> {
        let mut rendered = image.clone();
        for name in names {
            let look = self
                .looks
                .iter()
                .find(|candidate| candidate.name == *name)
                .expect("rule-set construction validates look references");
            rendered = look.apply(&rendered)?;
        }
        Ok(rendered)
    }
}

fn valid_name(name: String, operation: &'static str) -> Result<String> {
    if name.is_empty()
        || name.len() > MAX_NAME_BYTES
        || name.trim() != name
        || name.chars().any(char::is_control)
    {
        return Err(invalid(
            operation,
            "rule names must be trimmed, nonempty, bounded, and free of control characters",
        ));
    }
    Ok(name)
}

fn validate_references(references: &[String], operation: &'static str) -> Result<()> {
    for reference in references {
        valid_name(reference.clone(), operation)?;
    }
    validate_unique(
        references.iter().map(String::as_str),
        "look reference",
        operation,
    )
}

fn validate_registered_looks(
    looks: &[LookRule],
    references: &[String],
    owner_kind: &'static str,
    owner: &str,
) -> Result<()> {
    for reference in references {
        if !looks.iter().any(|look| look.name == *reference) {
            return Err(
                invalid("validate_rule_set", "rule references an unregistered look").with_context(
                    rule_context(owner_kind, owner).with_field("look", reference.clone()),
                ),
            );
        }
    }
    Ok(())
}

fn validate_unique<'a>(
    names: impl IntoIterator<Item = &'a str>,
    kind: &'static str,
    owner: &str,
) -> Result<()> {
    let names = names.into_iter().collect::<Vec<_>>();
    for (index, name) in names.iter().enumerate() {
        if names[..index].contains(name) {
            return Err(conflict("validate_rule_names", "rule names must be unique")
                .with_context(rule_context(kind, name).with_field("owner", owner.to_owned())));
        }
    }
    Ok(())
}

fn missing(operation: &'static str, kind: &'static str, name: &str) -> Error {
    invalid(operation, "requested color rule does not exist").with_context(rule_context(kind, name))
}

fn rule_context(kind: &'static str, name: &str) -> ErrorContext {
    ErrorContext::new(COMPONENT, "inspect_rule")
        .with_field("rule_kind", kind)
        .with_field("rule_name", name.to_owned())
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
