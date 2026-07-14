//! Deterministic 1D and 3D color lookup tables.
//!
//! This module parses the portable text subset of the `.cube` format: one
//! `LUT_1D_SIZE` or `LUT_3D_SIZE` declaration, optional `TITLE`, optional
//! `DOMAIN_MIN` and `DOMAIN_MAX`, followed by RGB table entries. Unknown
//! directives and combined shaper plus cube files are rejected instead of
//! guessed. Allocation is bounded before table data is accepted.
//!
//! LUT application never chooses an out-of-domain rule for the caller.
//! [`DomainPolicy::Reject`] retains extended working values by failing, while
//! [`DomainPolicy::Clamp`] makes clipping to the declared LUT domain explicit.
//! 3D tables use the `.cube` red-fastest entry order and expose both trilinear
//! and tetrahedral interpolation.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_image::value::ImageSamples;

use crate::working_space::WorkingImageF32;

const COMPONENT: &str = "superi-color.lut";
const MAX_1D_ENTRIES: usize = 65_536;
const MAX_3D_ENTRIES: usize = 2_000_000;
const MAX_SOURCE_BYTES: usize = 128 * 1024 * 1024;

/// Explicit behavior for RGB components outside a LUT's declared domain.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum DomainPolicy {
    /// Reject the application so extended values are not silently changed.
    Reject,
    /// Clamp each component to its declared domain before interpolation.
    Clamp,
}

/// Interpolation requested for one LUT application.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum LutInterpolation {
    /// Piecewise-linear interpolation for a 1D LUT.
    Linear,
    /// Eight-corner trilinear interpolation for a 3D LUT.
    Trilinear,
    /// Four-corner tetrahedral interpolation for a 3D LUT.
    Tetrahedral,
}

/// One parsed 1D RGB lookup table.
#[derive(Clone, Debug, PartialEq)]
pub struct Lut1D {
    title: Option<String>,
    domain_min: [f32; 3],
    domain_max: [f32; 3],
    entries: Vec<[f32; 3]>,
}

impl Lut1D {
    /// Returns the optional owned title from the `.cube` file.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Returns the independently declared RGB domain minimum.
    #[must_use]
    pub const fn domain_min(&self) -> [f32; 3] {
        self.domain_min
    }

    /// Returns the independently declared RGB domain maximum.
    #[must_use]
    pub const fn domain_max(&self) -> [f32; 3] {
        self.domain_max
    }

    /// Returns the number of samples in each channel curve.
    #[must_use]
    pub fn size(&self) -> usize {
        self.entries.len()
    }

    /// Returns the table entries in ascending input order.
    #[must_use]
    pub fn entries(&self) -> &[[f32; 3]] {
        &self.entries
    }

    /// Applies independent piecewise-linear interpolation to RGB components.
    pub fn apply(&self, rgb: [f32; 3], policy: DomainPolicy) -> Result<[f32; 3]> {
        let normalized = normalize_rgb(rgb, self.domain_min, self.domain_max, policy)?;
        let scale = (self.entries.len() - 1) as f64;
        let mut output = [0.0_f32; 3];
        for (channel, normalized) in normalized.into_iter().enumerate() {
            let coordinate = normalized * scale;
            let lower = coordinate.floor() as usize;
            let upper = lower.saturating_add(1).min(self.entries.len() - 1);
            output[channel] = interpolated_component(
                self.entries[lower][channel],
                self.entries[upper][channel],
                coordinate - lower as f64,
                "apply_1d_lut",
                channel,
            )?;
        }
        Ok(output)
    }
}

/// One parsed 3D RGB lookup table in `.cube` red-fastest order.
#[derive(Clone, Debug, PartialEq)]
pub struct Lut3D {
    title: Option<String>,
    domain_min: [f32; 3],
    domain_max: [f32; 3],
    size: usize,
    entries: Vec<[f32; 3]>,
}

impl Lut3D {
    /// Returns the optional owned title from the `.cube` file.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Returns the independently declared RGB domain minimum.
    #[must_use]
    pub const fn domain_min(&self) -> [f32; 3] {
        self.domain_min
    }

    /// Returns the independently declared RGB domain maximum.
    #[must_use]
    pub const fn domain_max(&self) -> [f32; 3] {
        self.domain_max
    }

    /// Returns the number of samples along each cube axis.
    #[must_use]
    pub const fn size(&self) -> usize {
        self.size
    }

    /// Returns entries in red-fastest, then green, then blue order.
    #[must_use]
    pub fn entries(&self) -> &[[f32; 3]] {
        &self.entries
    }

    /// Applies the requested 3D interpolation to an RGB value.
    pub fn apply(
        &self,
        rgb: [f32; 3],
        interpolation: LutInterpolation,
        policy: DomainPolicy,
    ) -> Result<[f32; 3]> {
        let normalized = normalize_rgb(rgb, self.domain_min, self.domain_max, policy)?;
        let coordinates = normalized.map(|component| component * (self.size - 1) as f64);
        let lower = coordinates.map(|coordinate| coordinate.floor() as usize);
        let upper = lower.map(|index| index.saturating_add(1).min(self.size - 1));
        let fraction = [
            coordinates[0] - lower[0] as f64,
            coordinates[1] - lower[1] as f64,
            coordinates[2] - lower[2] as f64,
        ];

        match interpolation {
            LutInterpolation::Trilinear => self.trilinear(lower, upper, fraction),
            LutInterpolation::Tetrahedral => self.tetrahedral(lower, upper, fraction),
            LutInterpolation::Linear => Err(invalid(
                "apply_3d_lut",
                "3D LUT application requires trilinear or tetrahedral interpolation",
            )),
        }
    }

    fn trilinear(
        &self,
        lower: [usize; 3],
        upper: [usize; 3],
        fraction: [f64; 3],
    ) -> Result<[f32; 3]> {
        let c000 = self.entry(lower[0], lower[1], lower[2]);
        let c100 = self.entry(upper[0], lower[1], lower[2]);
        let c010 = self.entry(lower[0], upper[1], lower[2]);
        let c110 = self.entry(upper[0], upper[1], lower[2]);
        let c001 = self.entry(lower[0], lower[1], upper[2]);
        let c101 = self.entry(upper[0], lower[1], upper[2]);
        let c011 = self.entry(lower[0], upper[1], upper[2]);
        let c111 = self.entry(upper[0], upper[1], upper[2]);

        let x00 = lerp_rgb(c000, c100, fraction[0]);
        let x10 = lerp_rgb(c010, c110, fraction[0]);
        let x01 = lerp_rgb(c001, c101, fraction[0]);
        let x11 = lerp_rgb(c011, c111, fraction[0]);
        let y0 = lerp_rgb_f64(x00, x10, fraction[1]);
        let y1 = lerp_rgb_f64(x01, x11, fraction[1]);
        checked_rgb_f32(lerp_rgb_f64(y0, y1, fraction[2]), "apply_trilinear_lut")
    }

    fn tetrahedral(
        &self,
        lower: [usize; 3],
        upper: [usize; 3],
        fraction: [f64; 3],
    ) -> Result<[f32; 3]> {
        let c000 = rgb_f64(self.entry(lower[0], lower[1], lower[2]));
        let c100 = rgb_f64(self.entry(upper[0], lower[1], lower[2]));
        let c010 = rgb_f64(self.entry(lower[0], upper[1], lower[2]));
        let c110 = rgb_f64(self.entry(upper[0], upper[1], lower[2]));
        let c001 = rgb_f64(self.entry(lower[0], lower[1], upper[2]));
        let c101 = rgb_f64(self.entry(upper[0], lower[1], upper[2]));
        let c011 = rgb_f64(self.entry(lower[0], upper[1], upper[2]));
        let c111 = rgb_f64(self.entry(upper[0], upper[1], upper[2]));
        let [red, green, blue] = fraction;

        let output = if red >= green {
            if green >= blue {
                add_three_scaled(c000, c100, red, c110, green, c111, blue)
            } else if red >= blue {
                add_three_scaled(c000, c100, red, c101, blue, c111, green)
            } else {
                add_three_scaled(c000, c001, blue, c101, red, c111, green)
            }
        } else if blue >= green {
            add_three_scaled(c000, c001, blue, c011, green, c111, red)
        } else if blue >= red {
            add_three_scaled(c000, c010, green, c011, blue, c111, red)
        } else {
            add_three_scaled(c000, c010, green, c110, red, c111, blue)
        };
        checked_rgb_f32(output, "apply_tetrahedral_lut")
    }

    fn entry(&self, red: usize, green: usize, blue: usize) -> [f32; 3] {
        let index = blue * self.size * self.size + green * self.size + red;
        self.entries[index]
    }
}

/// One parsed 1D or 3D `.cube` lookup table.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Lut {
    /// An independent RGB curve table.
    OneDimensional(Lut1D),
    /// A red-fastest RGB cube table.
    ThreeDimensional(Lut3D),
}

impl Lut {
    /// Parses one strict, bounded `.cube` text artifact.
    pub fn parse_cube(source: &str) -> Result<Self> {
        CubeParser::parse(source)
    }

    /// Returns the optional owned title from the `.cube` file.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        match self {
            Self::OneDimensional(lut) => lut.title(),
            Self::ThreeDimensional(lut) => lut.title(),
        }
    }

    /// Returns the independently declared RGB domain minimum.
    #[must_use]
    pub const fn domain_min(&self) -> [f32; 3] {
        match self {
            Self::OneDimensional(lut) => lut.domain_min(),
            Self::ThreeDimensional(lut) => lut.domain_min(),
        }
    }

    /// Returns the independently declared RGB domain maximum.
    #[must_use]
    pub const fn domain_max(&self) -> [f32; 3] {
        match self {
            Self::OneDimensional(lut) => lut.domain_max(),
            Self::ThreeDimensional(lut) => lut.domain_max(),
        }
    }

    /// Returns the curve length or the per-axis cube length.
    #[must_use]
    pub fn size(&self) -> usize {
        match self {
            Self::OneDimensional(lut) => lut.size(),
            Self::ThreeDimensional(lut) => lut.size(),
        }
    }

    /// Applies the LUT with an interpolation mode valid for its dimensionality.
    pub fn apply(
        &self,
        rgb: [f32; 3],
        interpolation: LutInterpolation,
        policy: DomainPolicy,
    ) -> Result<[f32; 3]> {
        match (self, interpolation) {
            (Self::OneDimensional(lut), LutInterpolation::Linear) => lut.apply(rgb, policy),
            (Self::OneDimensional(_), _) => Err(invalid(
                "apply_1d_lut",
                "1D LUT application requires linear interpolation",
            )),
            (Self::ThreeDimensional(lut), interpolation) => lut.apply(rgb, interpolation, policy),
        }
    }

    /// Applies this LUT to promoted scene-linear working pixels.
    ///
    /// Nonzero-alpha pixels are unpremultiplied before lookup and premultiplied
    /// afterward. A zero-alpha pixel retains its exact stored RGB payload
    /// because no straight color can be recovered from it. The image descriptor,
    /// windows, color tags, channel identities, alpha mode, and metadata remain
    /// unchanged.
    pub fn apply_to_working_image(
        &self,
        image: &WorkingImageF32,
        interpolation: LutInterpolation,
        policy: DomainPolicy,
    ) -> Result<WorkingImageF32> {
        let mut samples = (0..image.image().samples().len())
            .map(|index| {
                image.image().samples().float_value(index).ok_or_else(|| {
                    invalid(
                        "apply_lut_to_working_image",
                        "working image samples must use binary32 floating point",
                    )
                    .with_context(
                        ErrorContext::new(COMPONENT, "inspect_working_sample")
                            .with_field("sample_index", index.to_string()),
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?;

        for (pixel_index, pixel) in samples.chunks_exact_mut(4).enumerate() {
            let alpha = pixel[3];
            if alpha == 0.0 {
                continue;
            }
            if !alpha.is_finite() {
                return Err(invalid(
                    "apply_lut_to_working_image",
                    "nonzero working alpha must be finite during LUT application",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "inspect_working_alpha")
                        .with_field("pixel_index", pixel_index.to_string())
                        .with_field("alpha", alpha.to_string()),
                ));
            }
            let straight = [pixel[0] / alpha, pixel[1] / alpha, pixel[2] / alpha];
            let transformed = self
                .apply(straight, interpolation, policy)
                .map_err(|error| {
                    error.with_context(
                        ErrorContext::new(COMPONENT, "apply_working_pixel")
                            .with_field("pixel_index", pixel_index.to_string()),
                    )
                })?;
            for (channel, transformed) in transformed.into_iter().enumerate() {
                let premultiplied = f64::from(transformed) * f64::from(alpha);
                pixel[channel] = checked_f32(premultiplied, "apply_lut_to_working_image", channel)?;
            }
        }

        let transformed = image
            .image()
            .clone()
            .replace_samples(ImageSamples::from_f32(samples))?;
        WorkingImageF32::new(image.space(), transformed)
    }
}

#[derive(Clone, Copy, Debug)]
enum LutDeclaration {
    OneDimensional { size: usize },
    ThreeDimensional { size: usize, entries: usize },
}

impl LutDeclaration {
    const fn expected_entries(self) -> usize {
        match self {
            Self::OneDimensional { size } => size,
            Self::ThreeDimensional { entries, .. } => entries,
        }
    }
}

#[derive(Default)]
struct CubeParser {
    title: Option<String>,
    domain_min: Option<[f32; 3]>,
    domain_max: Option<[f32; 3]>,
    declaration: Option<LutDeclaration>,
    entries: Vec<[f32; 3]>,
    data_started: bool,
}

impl CubeParser {
    fn parse(source: &str) -> Result<Lut> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err(exhausted(
                "parse_cube",
                "LUT source exceeds the bounded parser input size",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "inspect_source_size")
                    .with_field("actual_bytes", source.len().to_string())
                    .with_field("maximum_bytes", MAX_SOURCE_BYTES.to_string()),
            ));
        }
        let source = source.strip_prefix('\u{feff}').unwrap_or(source);
        let mut parser = Self::default();
        for (line_index, raw_line) in source.lines().enumerate() {
            let line_number = line_index + 1;
            let content = strip_comment(raw_line).trim();
            if content.is_empty() {
                continue;
            }
            parser.parse_line(content, line_number)?;
        }
        parser.finish()
    }

    fn parse_line(&mut self, content: &str, line_number: usize) -> Result<()> {
        let first = content
            .split_whitespace()
            .next()
            .expect("nonempty content has a first token");
        let is_directive = matches!(
            first,
            "TITLE" | "DOMAIN_MIN" | "DOMAIN_MAX" | "LUT_1D_SIZE" | "LUT_3D_SIZE"
        );
        if self.data_started && is_directive {
            return Err(line_error(
                corrupt(
                    "parse_cube",
                    "LUT directives must precede all table entries",
                ),
                line_number,
                first,
            ));
        }

        match first {
            "TITLE" => self.parse_title(content, line_number),
            "DOMAIN_MIN" => self.parse_domain(content, line_number, true),
            "DOMAIN_MAX" => self.parse_domain(content, line_number, false),
            "LUT_1D_SIZE" => self.parse_declaration(content, line_number, true),
            "LUT_3D_SIZE" => self.parse_declaration(content, line_number, false),
            _ if first.parse::<f32>().is_ok() => self.parse_entry(content, line_number),
            _ => Err(line_error(
                unsupported("parse_cube", "unsupported or misspelled `.cube` directive"),
                line_number,
                first,
            )),
        }
    }

    fn parse_title(&mut self, content: &str, line_number: usize) -> Result<()> {
        if self.title.is_some() {
            return Err(line_error(
                corrupt("parse_cube", "`.cube` TITLE may appear only once"),
                line_number,
                "TITLE",
            ));
        }
        let value = content["TITLE".len()..].trim();
        if value.len() < 2
            || !value.starts_with('"')
            || !value.ends_with('"')
            || value[1..value.len() - 1].contains('"')
        {
            return Err(line_error(
                corrupt("parse_cube", "`.cube` TITLE must contain one quoted value"),
                line_number,
                "TITLE",
            ));
        }
        self.title = Some(value[1..value.len() - 1].to_owned());
        Ok(())
    }

    fn parse_domain(&mut self, content: &str, line_number: usize, minimum: bool) -> Result<()> {
        let target = if minimum {
            &mut self.domain_min
        } else {
            &mut self.domain_max
        };
        let directive = if minimum { "DOMAIN_MIN" } else { "DOMAIN_MAX" };
        if target.is_some() {
            return Err(line_error(
                corrupt("parse_cube", "a `.cube` domain directive is duplicated"),
                line_number,
                directive,
            ));
        }
        *target = Some(parse_triplet(content, directive, line_number)?);
        Ok(())
    }

    fn parse_declaration(
        &mut self,
        content: &str,
        line_number: usize,
        one_dimensional: bool,
    ) -> Result<()> {
        let directive = if one_dimensional {
            "LUT_1D_SIZE"
        } else {
            "LUT_3D_SIZE"
        };
        if self.declaration.is_some() {
            return Err(line_error(
                corrupt(
                    "parse_cube",
                    "a `.cube` file must declare exactly one LUT dimensionality",
                ),
                line_number,
                directive,
            ));
        }
        let size = parse_size(content, directive, line_number)?;
        if size < 2 {
            return Err(line_error(
                corrupt("parse_cube", "LUT axis size must be at least two"),
                line_number,
                directive,
            ));
        }
        let declaration = if one_dimensional {
            if size > MAX_1D_ENTRIES {
                return Err(size_exhausted(line_number, directive, size, MAX_1D_ENTRIES));
            }
            LutDeclaration::OneDimensional { size }
        } else {
            let entries = size
                .checked_mul(size)
                .and_then(|square| square.checked_mul(size))
                .ok_or_else(|| size_exhausted(line_number, directive, size, MAX_3D_ENTRIES))?;
            if entries > MAX_3D_ENTRIES {
                return Err(size_exhausted(
                    line_number,
                    directive,
                    entries,
                    MAX_3D_ENTRIES,
                ));
            }
            LutDeclaration::ThreeDimensional { size, entries }
        };
        let initial_capacity = declaration.expected_entries().min(4_096);
        self.entries
            .try_reserve_exact(initial_capacity)
            .map_err(|_| allocation_exhausted(line_number, directive, initial_capacity))?;
        self.declaration = Some(declaration);
        Ok(())
    }

    fn parse_entry(&mut self, content: &str, line_number: usize) -> Result<()> {
        let declaration = self.declaration.ok_or_else(|| {
            line_error(
                corrupt(
                    "parse_cube",
                    "LUT table data requires a preceding size declaration",
                ),
                line_number,
                "table_data",
            )
        })?;
        if self.entries.len() == declaration.expected_entries() {
            return Err(line_error(
                corrupt(
                    "parse_cube",
                    "LUT contains more table entries than declared",
                ),
                line_number,
                "table_data",
            ));
        }
        if self.entries.len() == self.entries.capacity() {
            let remaining = declaration.expected_entries() - self.entries.len();
            let additional = remaining.min(self.entries.capacity().max(4_096));
            self.entries
                .try_reserve_exact(additional)
                .map_err(|_| allocation_exhausted(line_number, "table_data", additional))?;
        }
        self.entries
            .push(parse_triplet(content, "table_data", line_number)?);
        self.data_started = true;
        Ok(())
    }

    fn finish(self) -> Result<Lut> {
        let declaration = self.declaration.ok_or_else(|| {
            corrupt(
                "parse_cube",
                "`.cube` source is missing a LUT size declaration",
            )
        })?;
        let expected = declaration.expected_entries();
        if self.entries.len() != expected {
            return Err(corrupt(
                "parse_cube",
                "LUT table entry count does not match its declared size",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "compare_entry_count")
                    .with_field("actual", self.entries.len().to_string())
                    .with_field("expected", expected.to_string()),
            ));
        }
        let domain_min = self.domain_min.unwrap_or([0.0; 3]);
        let domain_max = self.domain_max.unwrap_or([1.0; 3]);
        validate_domain(domain_min, domain_max)?;

        match declaration {
            LutDeclaration::OneDimensional { .. } => Ok(Lut::OneDimensional(Lut1D {
                title: self.title,
                domain_min,
                domain_max,
                entries: self.entries,
            })),
            LutDeclaration::ThreeDimensional { size, .. } => Ok(Lut::ThreeDimensional(Lut3D {
                title: self.title,
                domain_min,
                domain_max,
                size,
                entries: self.entries,
            })),
        }
    }
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (index, character) in line.char_indices() {
        match character {
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..index],
            _ => {}
        }
    }
    line
}

fn parse_size(content: &str, directive: &str, line_number: usize) -> Result<usize> {
    let mut fields = content.split_whitespace();
    let _ = fields.next();
    let value = fields.next().ok_or_else(|| {
        line_error(
            corrupt("parse_cube", "LUT size directive is missing its value"),
            line_number,
            directive,
        )
    })?;
    if fields.next().is_some() {
        return Err(line_error(
            corrupt("parse_cube", "LUT size directive has extra values"),
            line_number,
            directive,
        ));
    }
    value.parse::<usize>().map_err(|_| {
        line_error(
            corrupt("parse_cube", "LUT size is not a positive integer"),
            line_number,
            directive,
        )
    })
}

fn parse_triplet(content: &str, directive: &str, line_number: usize) -> Result<[f32; 3]> {
    let mut fields = content.split_whitespace();
    if directive != "table_data" {
        let _ = fields.next();
    }
    let mut output = [0.0_f32; 3];
    for output in &mut output {
        let field = fields.next().ok_or_else(|| {
            line_error(
                corrupt(
                    "parse_cube",
                    "LUT RGB data must contain exactly three values",
                ),
                line_number,
                directive,
            )
        })?;
        let value = field.parse::<f32>().map_err(|_| {
            line_error(
                corrupt("parse_cube", "LUT RGB data contains an invalid number"),
                line_number,
                directive,
            )
        })?;
        if !value.is_finite() {
            return Err(line_error(
                corrupt("parse_cube", "LUT RGB data must contain finite numbers"),
                line_number,
                directive,
            ));
        }
        *output = value;
    }
    if fields.next().is_some() {
        return Err(line_error(
            corrupt(
                "parse_cube",
                "LUT RGB data must contain exactly three values",
            ),
            line_number,
            directive,
        ));
    }
    Ok(output)
}

fn validate_domain(domain_min: [f32; 3], domain_max: [f32; 3]) -> Result<()> {
    for (channel, (minimum, maximum)) in domain_min.into_iter().zip(domain_max).enumerate() {
        if minimum >= maximum {
            return Err(corrupt(
                "parse_cube",
                "each LUT domain maximum must be greater than its minimum",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "validate_domain")
                    .with_field("channel", channel_name(channel))
                    .with_field("minimum", minimum.to_string())
                    .with_field("maximum", maximum.to_string()),
            ));
        }
    }
    Ok(())
}

fn normalize_rgb(
    rgb: [f32; 3],
    domain_min: [f32; 3],
    domain_max: [f32; 3],
    policy: DomainPolicy,
) -> Result<[f64; 3]> {
    let mut normalized = [0.0_f64; 3];
    for (channel, value) in rgb.into_iter().enumerate() {
        if !value.is_finite() {
            return Err(
                invalid("apply_lut", "LUT input components must be finite").with_context(
                    ErrorContext::new(COMPONENT, "inspect_input")
                        .with_field("channel", channel_name(channel))
                        .with_field("value", value.to_string()),
                ),
            );
        }
        let minimum = domain_min[channel];
        let maximum = domain_max[channel];
        let bounded = match policy {
            DomainPolicy::Reject => {
                if value < minimum || value > maximum {
                    return Err(invalid(
                        "apply_lut",
                        "LUT input falls outside the declared domain",
                    )
                    .with_context(
                        ErrorContext::new(COMPONENT, "inspect_input_domain")
                            .with_field("channel", channel_name(channel))
                            .with_field("value", value.to_string())
                            .with_field("minimum", minimum.to_string())
                            .with_field("maximum", maximum.to_string()),
                    ));
                }
                value
            }
            DomainPolicy::Clamp => value.clamp(minimum, maximum),
        };
        normalized[channel] =
            (f64::from(bounded) - f64::from(minimum)) / (f64::from(maximum) - f64::from(minimum));
    }
    Ok(normalized)
}

fn interpolated_component(
    lower: f32,
    upper: f32,
    fraction: f64,
    operation: &'static str,
    channel: usize,
) -> Result<f32> {
    checked_f32(
        f64::from(lower) + fraction * (f64::from(upper) - f64::from(lower)),
        operation,
        channel,
    )
}

fn lerp_rgb(lower: [f32; 3], upper: [f32; 3], fraction: f64) -> [f64; 3] {
    [
        f64::from(lower[0]) + fraction * (f64::from(upper[0]) - f64::from(lower[0])),
        f64::from(lower[1]) + fraction * (f64::from(upper[1]) - f64::from(lower[1])),
        f64::from(lower[2]) + fraction * (f64::from(upper[2]) - f64::from(lower[2])),
    ]
}

fn lerp_rgb_f64(lower: [f64; 3], upper: [f64; 3], fraction: f64) -> [f64; 3] {
    [
        lower[0] + fraction * (upper[0] - lower[0]),
        lower[1] + fraction * (upper[1] - lower[1]),
        lower[2] + fraction * (upper[2] - lower[2]),
    ]
}

fn rgb_f64(value: [f32; 3]) -> [f64; 3] {
    value.map(f64::from)
}

fn add_three_scaled(
    base: [f64; 3],
    first: [f64; 3],
    first_scale: f64,
    second: [f64; 3],
    second_scale: f64,
    third: [f64; 3],
    third_scale: f64,
) -> [f64; 3] {
    let first_delta = subtract_rgb(first, base);
    let second_delta = subtract_rgb(second, first);
    let third_delta = subtract_rgb(third, second);
    [
        base[0]
            + first_scale * first_delta[0]
            + second_scale * second_delta[0]
            + third_scale * third_delta[0],
        base[1]
            + first_scale * first_delta[1]
            + second_scale * second_delta[1]
            + third_scale * third_delta[1],
        base[2]
            + first_scale * first_delta[2]
            + second_scale * second_delta[2]
            + third_scale * third_delta[2],
    ]
}

fn subtract_rgb(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn checked_rgb_f32(value: [f64; 3], operation: &'static str) -> Result<[f32; 3]> {
    Ok([
        checked_f32(value[0], operation, 0)?,
        checked_f32(value[1], operation, 1)?,
        checked_f32(value[2], operation, 2)?,
    ])
}

fn checked_f32(value: f64, operation: &'static str, channel: usize) -> Result<f32> {
    let narrowed = value as f32;
    if !value.is_finite() || !narrowed.is_finite() {
        return Err(invalid(
            operation,
            "LUT interpolation produced a nonfinite component",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "inspect_output")
                .with_field("channel", channel_name(channel))
                .with_field("value", value.to_string()),
        ));
    }
    Ok(narrowed)
}

const fn channel_name(channel: usize) -> &'static str {
    match channel {
        0 => "R",
        1 => "G",
        2 => "B",
        _ => "unknown",
    }
}

fn line_error(error: Error, line_number: usize, directive: &str) -> Error {
    error.with_context(
        ErrorContext::new(COMPONENT, "inspect_cube_line")
            .with_field("line", line_number.to_string())
            .with_field("directive", directive),
    )
}

fn size_exhausted(line_number: usize, directive: &str, requested: usize, maximum: usize) -> Error {
    line_error(
        exhausted(
            "parse_cube",
            "declared LUT size exceeds the bounded entry limit",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "inspect_declared_size")
                .with_field("requested", requested.to_string())
                .with_field("maximum", maximum.to_string()),
        ),
        line_number,
        directive,
    )
}

fn allocation_exhausted(line_number: usize, directive: &str, entries: usize) -> Error {
    line_error(
        exhausted("parse_cube", "LUT table allocation could not be satisfied").with_context(
            ErrorContext::new(COMPONENT, "allocate_table")
                .with_field("additional_entries", entries.to_string()),
        ),
        line_number,
        directive,
    )
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn corrupt(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn exhausted(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
