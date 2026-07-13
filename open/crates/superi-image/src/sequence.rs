//! Deterministic filesystem image-sequence discovery and frame resolution.
//!
//! A logical image number is a zero-based position in a manifest. A file-frame
//! number is the signed decimal label embedded in a filename. The distinction
//! is retained through gaps, explicit frame steps, and missing-frame policy so
//! callers never have to infer editorial timing from directory enumeration.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

const COMPONENT: &str = "superi-image.sequence";

/// A filesystem filename pattern with one signed decimal frame number.
///
/// `zero_padding` counts digits and excludes a negative sign. It is a minimum
/// width, so frame numbers that naturally need more digits remain representable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageSequencePattern {
    directory: PathBuf,
    prefix: String,
    suffix: String,
    zero_padding: usize,
}

impl ImageSequencePattern {
    /// Creates an explicit sequence pattern.
    pub fn new(
        directory: PathBuf,
        prefix: impl Into<String>,
        suffix: impl Into<String>,
        zero_padding: usize,
    ) -> Result<Self> {
        let prefix = prefix.into();
        let suffix = suffix.into();
        if zero_padding == 0 {
            return Err(invalid(
                "create_sequence_pattern",
                "image sequence zero padding must be greater than zero",
            ));
        }
        if prefix.contains(['/', '\\']) || suffix.contains(['/', '\\']) {
            return Err(invalid(
                "create_sequence_pattern",
                "image sequence prefix and suffix must not contain path separators",
            ));
        }
        Ok(Self {
            directory,
            prefix,
            suffix,
            zero_padding,
        })
    }

    /// Parses the rightmost signed decimal run in a selected frame path.
    pub fn parse_frame_path(path: impl AsRef<Path>) -> Result<ParsedImageSequencePath> {
        let path = path.as_ref();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                invalid_with_path(
                    "parse_sequence_path",
                    "image sequence frame path must have a UTF-8 filename",
                    path,
                )
            })?;
        let bytes = file_name.as_bytes();
        let digit_end = bytes.iter().rposition(u8::is_ascii_digit).ok_or_else(|| {
            invalid_with_path(
                "parse_sequence_path",
                "image sequence frame filename has no decimal frame number",
                path,
            )
        })?;
        let mut digit_start = digit_end;
        while digit_start > 0 && bytes[digit_start - 1].is_ascii_digit() {
            digit_start -= 1;
        }
        let number_start = if digit_start > 0 && bytes[digit_start - 1] == b'-' {
            digit_start - 1
        } else {
            digit_start
        };
        let number_text = &file_name[number_start..=digit_end];
        let frame_number = number_text.parse::<i64>().map_err(|error| {
            Error::with_source(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "image sequence frame number is outside the supported signed range",
                error,
            )
            .with_context(path_context("parse_sequence_path", path))
        })?;
        let directory = path.parent().unwrap_or_else(|| Path::new("")).to_path_buf();
        let pattern = Self::new(
            directory,
            &file_name[..number_start],
            &file_name[digit_end + 1..],
            digit_end - digit_start + 1,
        )?;
        if pattern.format_frame_number(frame_number) != number_text {
            return Err(invalid_with_path(
                "parse_sequence_path",
                "image sequence frame number does not use canonical zero padding",
                path,
            ));
        }
        Ok(ParsedImageSequencePath {
            pattern,
            frame_number,
        })
    }

    /// Returns the directory containing sequence frames.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Returns filename text before the signed decimal frame number.
    #[must_use]
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Returns filename text after the signed decimal frame number.
    #[must_use]
    pub fn suffix(&self) -> &str {
        &self.suffix
    }

    /// Returns the minimum number of digits, excluding a negative sign.
    #[must_use]
    pub const fn zero_padding(&self) -> usize {
        self.zero_padding
    }

    /// Resolves one signed file-frame number to its deterministic path.
    pub fn path_for_frame(&self, frame_number: i64) -> Result<PathBuf> {
        let number = self.format_frame_number(frame_number);
        let capacity = self
            .prefix
            .len()
            .checked_add(number.len())
            .and_then(|length| length.checked_add(self.suffix.len()))
            .ok_or_else(|| exhausted("format_sequence_path", "sequence filename is too large"))?;
        let mut file_name = String::with_capacity(capacity);
        file_name.push_str(&self.prefix);
        file_name.push_str(&number);
        file_name.push_str(&self.suffix);
        Ok(self.directory.join(file_name))
    }

    fn format_frame_number(&self, frame_number: i64) -> String {
        let magnitude = frame_number.unsigned_abs();
        if frame_number < 0 {
            format!("-{magnitude:0>width$}", width = self.zero_padding)
        } else {
            format!("{magnitude:0>width$}", width = self.zero_padding)
        }
    }

    fn match_path(&self, path: &Path) -> Result<Option<i64>> {
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            return Ok(None);
        };
        let Some(remainder) = file_name.strip_prefix(&self.prefix) else {
            return Ok(None);
        };
        let Some(number_text) = remainder.strip_suffix(&self.suffix) else {
            return Ok(None);
        };
        let digits = number_text.strip_prefix('-').unwrap_or(number_text);
        if digits.len() < self.zero_padding
            || digits.is_empty()
            || !digits.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Ok(None);
        }
        let frame_number = number_text.parse::<i64>().map_err(|error| {
            Error::with_source(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "matching image sequence filename has an unrepresentable frame number",
                error,
            )
            .with_context(path_context("match_sequence_path", path))
        })?;
        if self.format_frame_number(frame_number) != number_text {
            return Ok(None);
        }
        Ok(Some(frame_number))
    }
}

/// A selected frame path split into an immutable pattern and signed label.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedImageSequencePath {
    pattern: ImageSequencePattern,
    frame_number: i64,
}

impl ParsedImageSequencePath {
    /// Returns the parsed filename pattern.
    #[must_use]
    pub const fn pattern(&self) -> &ImageSequencePattern {
        &self.pattern
    }

    /// Returns the signed label embedded in the selected filename.
    #[must_use]
    pub const fn frame_number(&self) -> i64 {
        self.frame_number
    }

    /// Consumes the parsed path and returns its pattern.
    #[must_use]
    pub fn into_pattern(self) -> ImageSequencePattern {
        self.pattern
    }
}

/// One zero-based logical position and its optional concrete file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageSequenceFrame {
    image_number: u64,
    file_frame_number: i64,
    path: Option<PathBuf>,
}

impl ImageSequenceFrame {
    /// Returns the stable zero-based logical image number.
    #[must_use]
    pub const fn image_number(&self) -> u64 {
        self.image_number
    }

    /// Returns the signed label expected in the frame filename.
    #[must_use]
    pub const fn file_frame_number(&self) -> i64 {
        self.file_frame_number
    }

    /// Returns the discovered file path, or `None` when this slot is missing.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

/// Deterministic discovered files and gaps for one explicit numbering range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageSequenceManifest {
    pattern: ImageSequencePattern,
    first_frame_number: i64,
    last_frame_number: i64,
    frame_step: u32,
    frames: Vec<ImageSequenceFrame>,
    available_frame_count: usize,
    missing_frame_numbers: Vec<i64>,
}

impl ImageSequenceManifest {
    /// Discovers every matching file and uses the observed minimum and maximum.
    ///
    /// `frame_step` is explicit. Gaps are never used to infer a larger step.
    pub fn discover(selected_frame: impl AsRef<Path>, frame_step: u32) -> Result<Self> {
        require_step(frame_step)?;
        let selected_frame = selected_frame.as_ref();
        let metadata = fs::metadata(selected_frame)
            .map_err(|error| filesystem_error("inspect_selected_frame", selected_frame, error))?;
        if !metadata.is_file() {
            return Err(not_found(
                "inspect_selected_frame",
                "selected image sequence frame is not a file",
                selected_frame,
                None,
            ));
        }
        let pattern = ImageSequencePattern::parse_frame_path(selected_frame)?.into_pattern();
        let discovered = scan_pattern(&pattern)?;
        let Some((&first_frame_number, _)) = discovered.first_key_value() else {
            return Err(not_found(
                "discover_sequence",
                "image sequence pattern matched no files",
                pattern.directory(),
                None,
            ));
        };
        let last_frame_number = *discovered
            .last_key_value()
            .expect("nonempty discovered frame map")
            .0;
        if (i128::from(last_frame_number) - i128::from(first_frame_number)) % i128::from(frame_step)
            != 0
        {
            return Err(conflict(
                "validate_sequence_numbering",
                "discovered frame does not align with the explicit sequence step",
                &pattern,
                last_frame_number,
            ));
        }
        Self::from_discovered(
            pattern,
            first_frame_number,
            last_frame_number,
            frame_step,
            discovered,
        )
    }

    /// Discovers matching files inside one explicit inclusive frame range.
    pub fn discover_range(
        pattern: ImageSequencePattern,
        first_frame_number: i64,
        last_frame_number: i64,
        frame_step: u32,
    ) -> Result<Self> {
        require_step(frame_step)?;
        validate_range(first_frame_number, last_frame_number, frame_step)?;
        let discovered = scan_pattern(&pattern)?
            .into_iter()
            .filter(|(number, _)| *number >= first_frame_number && *number <= last_frame_number)
            .collect::<BTreeMap<_, _>>();
        if discovered.is_empty() {
            return Err(not_found(
                "discover_sequence_range",
                "image sequence range contains no available frames",
                pattern.directory(),
                Some(first_frame_number),
            ));
        }
        Self::from_discovered(
            pattern,
            first_frame_number,
            last_frame_number,
            frame_step,
            discovered,
        )
    }

    fn from_discovered(
        pattern: ImageSequencePattern,
        first_frame_number: i64,
        last_frame_number: i64,
        frame_step: u32,
        discovered: BTreeMap<i64, PathBuf>,
    ) -> Result<Self> {
        validate_range(first_frame_number, last_frame_number, frame_step)?;
        let step = i128::from(frame_step);
        for frame_number in discovered.keys() {
            let offset = i128::from(*frame_number) - i128::from(first_frame_number);
            if offset < 0
                || i128::from(*frame_number) > i128::from(last_frame_number)
                || offset % step != 0
            {
                return Err(conflict(
                    "validate_sequence_numbering",
                    "discovered frame does not align with the explicit sequence range and step",
                    &pattern,
                    *frame_number,
                ));
            }
        }

        let span = i128::from(last_frame_number) - i128::from(first_frame_number);
        let logical_count = span
            .checked_div(step)
            .and_then(|count| count.checked_add(1))
            .ok_or_else(|| {
                exhausted(
                    "build_sequence_manifest",
                    "image sequence logical frame count overflowed",
                )
            })?;
        let logical_count = usize::try_from(logical_count).map_err(|_| {
            exhausted(
                "build_sequence_manifest",
                "image sequence logical frame count exceeds platform capacity",
            )
        })?;
        let mut frames = Vec::new();
        frames.try_reserve_exact(logical_count).map_err(|error| {
            Error::with_source(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "image sequence manifest allocation failed",
                error,
            )
            .with_context(pattern_context("build_sequence_manifest", &pattern))
        })?;
        let mut missing_frame_numbers = Vec::new();
        missing_frame_numbers
            .try_reserve_exact(logical_count.saturating_sub(discovered.len()))
            .map_err(|error| {
                Error::with_source(
                    ErrorCategory::ResourceExhausted,
                    Recoverability::UserCorrectable,
                    "image sequence gap allocation failed",
                    error,
                )
                .with_context(pattern_context("build_sequence_manifest", &pattern))
            })?;
        for image_index in 0..logical_count {
            let image_number = u64::try_from(image_index).map_err(|_| {
                exhausted(
                    "build_sequence_manifest",
                    "image sequence logical index exceeds supported range",
                )
            })?;
            let frame_number = checked_frame_number(
                first_frame_number,
                frame_step,
                image_number,
                "build_sequence_manifest",
            )?;
            let path = discovered.get(&frame_number).cloned();
            if path.is_none() {
                missing_frame_numbers.push(frame_number);
            }
            frames.push(ImageSequenceFrame {
                image_number,
                file_frame_number: frame_number,
                path,
            });
        }
        Ok(Self {
            pattern,
            first_frame_number,
            last_frame_number,
            frame_step,
            frames,
            available_frame_count: discovered.len(),
            missing_frame_numbers,
        })
    }

    /// Returns the immutable filename pattern.
    #[must_use]
    pub const fn pattern(&self) -> &ImageSequencePattern {
        &self.pattern
    }

    /// Returns the first signed file-frame label.
    #[must_use]
    pub const fn first_frame_number(&self) -> i64 {
        self.first_frame_number
    }

    /// Returns the last signed file-frame label.
    #[must_use]
    pub const fn last_frame_number(&self) -> i64 {
        self.last_frame_number
    }

    /// Returns the positive difference between adjacent file-frame labels.
    #[must_use]
    pub const fn frame_step(&self) -> u32 {
        self.frame_step
    }

    /// Returns the number of logical slots, including missing files.
    #[must_use]
    pub fn logical_frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Returns the number of discovered concrete frame files.
    #[must_use]
    pub const fn available_frame_count(&self) -> usize {
        self.available_frame_count
    }

    /// Returns missing signed labels in logical order.
    #[must_use]
    pub fn missing_frame_numbers(&self) -> &[i64] {
        &self.missing_frame_numbers
    }

    /// Iterates every logical position in stable order.
    pub fn frames(&self) -> impl ExactSizeIterator<Item = &ImageSequenceFrame> {
        self.frames.iter()
    }

    /// Returns one zero-based logical position.
    pub fn frame(&self, image_number: u64) -> Result<&ImageSequenceFrame> {
        let index = usize::try_from(image_number).map_err(|_| {
            invalid_with_image(
                "resolve_sequence_frame",
                "logical image number exceeds platform capacity",
                image_number,
            )
        })?;
        self.frames.get(index).ok_or_else(|| {
            invalid_with_image(
                "resolve_sequence_frame",
                "logical image number is outside the sequence",
                image_number,
            )
        })
    }

    /// Resolves one logical slot under an explicit missing-frame policy.
    pub fn resolve(
        &self,
        image_number: u64,
        policy: MissingFramePolicy,
    ) -> Result<ResolvedSequenceFrame> {
        let requested = self.frame(image_number)?.clone();
        if let Some(path) = requested.path.clone() {
            let source_frame_number = requested.file_frame_number;
            return Ok(ResolvedSequenceFrame {
                requested,
                source_frame_number: Some(source_frame_number),
                read_path: Some(path),
                reference_path: None,
                substitution: SequenceSubstitution::None,
            });
        }
        let missing_path = self.pattern.path_for_frame(requested.file_frame_number)?;
        match policy {
            MissingFramePolicy::Error => Err(not_found(
                "resolve_missing_frame",
                "image sequence frame file is missing",
                &missing_path,
                Some(requested.file_frame_number),
            )),
            MissingFramePolicy::Hold => {
                let source = self.frames[..usize::try_from(image_number).expect("validated index")]
                    .iter()
                    .rev()
                    .find(|frame| frame.path.is_some())
                    .ok_or_else(|| {
                        not_found(
                            "resolve_missing_frame_hold",
                            "missing image sequence frame has no earlier frame to hold",
                            &missing_path,
                            Some(requested.file_frame_number),
                        )
                    })?;
                Ok(ResolvedSequenceFrame {
                    requested,
                    source_frame_number: Some(source.file_frame_number),
                    read_path: source.path.clone(),
                    reference_path: None,
                    substitution: SequenceSubstitution::Hold,
                })
            }
            MissingFramePolicy::Black => {
                let index = usize::try_from(image_number).expect("validated index");
                let reference = self.frames[..index]
                    .iter()
                    .rev()
                    .chain(self.frames[index + 1..].iter())
                    .find(|frame| frame.path.is_some())
                    .ok_or_else(|| {
                        not_found(
                            "resolve_missing_frame_black",
                            "missing image sequence frame has no image to define black-frame semantics",
                            &missing_path,
                            Some(requested.file_frame_number),
                        )
                    })?;
                Ok(ResolvedSequenceFrame {
                    requested,
                    source_frame_number: None,
                    read_path: None,
                    reference_path: reference.path.clone(),
                    substitution: SequenceSubstitution::Black,
                })
            }
        }
    }
}

/// Missing-file behavior compatible with editorial image-sequence references.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum MissingFramePolicy {
    /// Return a user-correctable not-found error.
    Error,
    /// Reuse the last available earlier frame.
    Hold,
    /// Produce black using a nearby available frame's exact image semantics.
    Black,
}

/// How a resolved logical slot obtains its image.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum SequenceSubstitution {
    /// The requested file exists.
    None,
    /// An earlier available file is held.
    Hold,
    /// A semantic black image is generated.
    Black,
}

/// Concrete resolution of one requested logical sequence position.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedSequenceFrame {
    requested: ImageSequenceFrame,
    source_frame_number: Option<i64>,
    read_path: Option<PathBuf>,
    reference_path: Option<PathBuf>,
    substitution: SequenceSubstitution,
}

impl ResolvedSequenceFrame {
    /// Returns the requested logical and file-frame coordinates.
    #[must_use]
    pub const fn requested(&self) -> &ImageSequenceFrame {
        &self.requested
    }

    /// Returns the file-frame label actually read for exact and held frames.
    #[must_use]
    pub const fn source_frame_number(&self) -> Option<i64> {
        self.source_frame_number
    }

    /// Returns the concrete path to decode for exact and held frames.
    #[must_use]
    pub fn read_path(&self) -> Option<&Path> {
        self.read_path.as_deref()
    }

    /// Returns the image used only to define semantic black output.
    #[must_use]
    pub fn reference_path(&self) -> Option<&Path> {
        self.reference_path.as_deref()
    }

    /// Returns the applied substitution behavior.
    #[must_use]
    pub const fn substitution(&self) -> SequenceSubstitution {
        self.substitution
    }
}

fn scan_pattern(pattern: &ImageSequencePattern) -> Result<BTreeMap<i64, PathBuf>> {
    let directory = if pattern.directory().as_os_str().is_empty() {
        Path::new(".")
    } else {
        pattern.directory()
    };
    let entries = fs::read_dir(directory)
        .map_err(|error| filesystem_error("read_sequence_directory", directory, error))?;
    let mut discovered = BTreeMap::new();
    for entry in entries {
        let entry = entry
            .map_err(|error| filesystem_error("read_sequence_directory_entry", directory, error))?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|error| filesystem_error("inspect_sequence_file", &path, error))?;
        if !metadata.is_file() {
            continue;
        }
        let Some(frame_number) = pattern.match_path(&path)? else {
            continue;
        };
        if let Some(existing) = discovered.insert(frame_number, path.clone()) {
            return Err(conflict(
                "discover_sequence",
                "multiple files resolve to the same image sequence frame number",
                pattern,
                frame_number,
            )
            .with_context(
                ErrorContext::new(COMPONENT, "compare_sequence_paths")
                    .with_field("first_path", existing.display().to_string())
                    .with_field("second_path", path.display().to_string()),
            ));
        }
    }
    Ok(discovered)
}

fn require_step(frame_step: u32) -> Result<()> {
    if frame_step == 0 {
        return Err(invalid(
            "validate_sequence_step",
            "image sequence frame step must be greater than zero",
        ));
    }
    Ok(())
}

fn validate_range(first: i64, last: i64, step: u32) -> Result<()> {
    require_step(step)?;
    if first > last {
        return Err(invalid(
            "validate_sequence_range",
            "image sequence first frame must not exceed its last frame",
        ));
    }
    let span = i128::from(last) - i128::from(first);
    if span % i128::from(step) != 0 {
        return Err(invalid(
            "validate_sequence_range",
            "image sequence last frame does not align with its first frame and step",
        ));
    }
    Ok(())
}

fn checked_frame_number(
    first: i64,
    step: u32,
    image_number: u64,
    operation: &'static str,
) -> Result<i64> {
    let number = i128::from(step)
        .checked_mul(i128::from(image_number))
        .and_then(|offset| offset.checked_add(i128::from(first)))
        .ok_or_else(|| exhausted(operation, "image sequence frame number overflowed"))?;
    i64::try_from(number).map_err(|_| {
        exhausted(
            operation,
            "image sequence frame number exceeds the supported signed range",
        )
    })
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn invalid_with_path(operation: &'static str, message: &'static str, path: &Path) -> Error {
    invalid(operation, message).with_context(path_context(operation, path))
}

fn invalid_with_image(operation: &'static str, message: &'static str, image_number: u64) -> Error {
    invalid(operation, message).with_context(
        ErrorContext::new(COMPONENT, "inspect_sequence_address")
            .with_field("image_number", image_number.to_string()),
    )
}

fn conflict(
    operation: &'static str,
    message: &'static str,
    pattern: &ImageSequencePattern,
    frame_number: i64,
) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        pattern_context(operation, pattern).with_field("file_frame", frame_number.to_string()),
    )
}

fn not_found(
    operation: &'static str,
    message: &'static str,
    path: &Path,
    frame_number: Option<i64>,
) -> Error {
    let mut context = path_context(operation, path);
    if let Some(frame_number) = frame_number {
        context.insert_field("file_frame", frame_number.to_string());
    }
    Error::new(
        ErrorCategory::NotFound,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(context)
}

fn exhausted(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn path_context(operation: &'static str, path: &Path) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation).with_field("path", path.display().to_string())
}

fn pattern_context(operation: &'static str, pattern: &ImageSequencePattern) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation)
        .with_field("directory", pattern.directory.display().to_string())
        .with_field("prefix", pattern.prefix.clone())
        .with_field("suffix", pattern.suffix.clone())
        .with_field("zero_padding", pattern.zero_padding.to_string())
}

fn filesystem_error(operation: &'static str, path: &Path, source: io::Error) -> Error {
    let (category, recoverability, message) = match source.kind() {
        io::ErrorKind::NotFound => (
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            "image sequence filesystem path was not found",
        ),
        io::ErrorKind::PermissionDenied => (
            ErrorCategory::PermissionDenied,
            Recoverability::UserCorrectable,
            "image sequence filesystem access was denied",
        ),
        io::ErrorKind::InvalidInput => (
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "image sequence filesystem path is invalid",
        ),
        _ => (
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            "image sequence filesystem access failed",
        ),
    };
    Error::with_source(category, recoverability, message, source)
        .with_context(path_context(operation, path))
}
