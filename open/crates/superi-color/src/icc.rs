//! ICC display-profile discovery and monitor-aware presentation state.
//!
//! Profiles are untrusted inputs. This module validates a bounded ICC display
//! profile before retaining its bytes, gives the artifact a content identity,
//! and publishes immutable display snapshots. Presentation bindings retain the
//! exact profile identity selected for one monitor, so a display move or system
//! profile change is observable before another frame is presented.
//!
//! ICC transform evaluation remains in the color transform pipeline. This
//! module owns discovery, structure, identity, selection, and invalidation.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

#[cfg(target_os = "macos")]
mod macos;

const COMPONENT: &str = "superi-color.icc";
const ICC_HEADER_BYTES: usize = 128;
const ICC_TAG_COUNT_BYTES: usize = 4;
const ICC_TAG_ENTRY_BYTES: usize = 12;

/// Maximum accepted ICC profile size, including its header and tag data.
pub const MAX_ICC_PROFILE_BYTES: usize = 16 * 1024 * 1024;

/// Maximum number of tags accepted from one ICC profile.
pub const MAX_ICC_TAGS: usize = 4_096;

/// Maximum number of active displays retained in one discovery snapshot.
pub const MAX_ACTIVE_DISPLAYS: usize = 64;

/// SHA-256 content identity of one complete validated ICC profile.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IccProfileId([u8; 32]);

impl IccProfileId {
    /// Returns the complete digest bytes.
    #[must_use]
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Display for IccProfileId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// ICC profile-format version declared by the header.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct IccVersion {
    major: u8,
    minor: u8,
    bugfix: u8,
}

impl IccVersion {
    /// Returns the major profile-format version.
    #[must_use]
    pub const fn major(self) -> u8 {
        self.major
    }

    /// Returns the minor profile-format version.
    #[must_use]
    pub const fn minor(self) -> u8 {
        self.minor
    }

    /// Returns the bug-fix profile-format version.
    #[must_use]
    pub const fn bugfix(self) -> u8 {
        self.bugfix
    }
}

/// ICC profile/device class supported by this display boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum IccProfileClass {
    /// Display device profile (`mntr`).
    Display,
}

/// ICC device or profile connection color space.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum IccColorSpace {
    /// Three-component device RGB (`RGB `).
    Rgb,
    /// CIE XYZ profile connection space (`XYZ `).
    Xyz,
    /// CIE Lab profile connection space (`Lab `).
    Lab,
}

/// ICC rendering intent declared by the profile header.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum IccRenderingIntent {
    /// Perceptual rendering intent.
    Perceptual,
    /// Media-relative colorimetric rendering intent.
    MediaRelativeColorimetric,
    /// Saturation rendering intent.
    Saturation,
    /// ICC-absolute colorimetric rendering intent.
    AbsoluteColorimetric,
}

/// Complete ICC transform model supplied by an RGB display profile.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum IccDisplayModel {
    /// Three-component matrix columns and tone reproduction curves.
    MatrixTrc,
    /// Paired device-to-PCS and PCS-to-device lookup transforms.
    Lut,
}

/// One validated ICC tag directory entry and its exact shared profile bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IccTag {
    signature: [u8; 4],
    profile_bytes: Arc<[u8]>,
    offset: usize,
    size: usize,
}

impl IccTag {
    /// Returns the four-byte ICC tag signature.
    #[must_use]
    pub const fn signature(&self) -> [u8; 4] {
        self.signature
    }

    /// Returns the exact validated tag payload without copying it.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.profile_bytes[self.offset..self.offset + self.size]
    }

    /// Returns the four-byte ICC tag data type signature.
    #[must_use]
    pub fn data_type_signature(&self) -> [u8; 4] {
        self.data()[0..4]
            .try_into()
            .expect("validated ICC tag data type has four bytes")
    }
}

/// Immutable, structurally validated RGB display profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IccProfile {
    id: IccProfileId,
    bytes: Arc<[u8]>,
    version: IccVersion,
    class: IccProfileClass,
    data_color_space: IccColorSpace,
    connection_space: IccColorSpace,
    rendering_intent: IccRenderingIntent,
    display_model: IccDisplayModel,
    tags: Arc<[IccTag]>,
}

impl IccProfile {
    /// Parses a bounded ICC v2 or v4 RGB display profile.
    ///
    /// The complete byte length, `acsp` signature, device class, color spaces,
    /// rendering intent, tag count, unique tag signatures, and every tag range
    /// are validated before the profile becomes visible to a caller.
    pub fn parse(bytes: impl Into<Arc<[u8]>>) -> Result<Self> {
        let bytes = bytes.into();
        validate_profile_length(bytes.len())?;

        let declared_size = read_u32(&bytes, 0, "read_profile_size")? as usize;
        if declared_size != bytes.len() {
            return Err(corrupt(
                "validate_profile_size",
                "ICC profile size does not match the complete byte buffer",
            )
            .with_context(
                profile_context("validate_profile_size")
                    .with_field("declared_size", declared_size.to_string())
                    .with_field("actual_size", bytes.len().to_string()),
            ));
        }
        if bytes[36..40] != *b"acsp" {
            return Err(corrupt(
                "validate_profile_signature",
                "ICC profile is missing the required acsp signature",
            ));
        }
        validate_header(&bytes)?;

        let version = parse_version(&bytes)?;
        let class = parse_profile_class(&bytes[12..16])?;
        let data_color_space = parse_data_color_space(&bytes[16..20])?;
        let connection_space = parse_connection_space(&bytes[20..24])?;
        let rendering_intent =
            parse_rendering_intent(read_u32(&bytes, 64, "read_rendering_intent")?)?;
        let tags = parse_tags(Arc::clone(&bytes))?;
        let display_model = validate_required_display_tags(&tags, connection_space)?;

        let digest: [u8; 32] = Sha256::digest(&bytes).into();
        Ok(Self {
            id: IccProfileId(digest),
            bytes,
            version,
            class,
            data_color_space,
            connection_space,
            rendering_intent,
            display_model,
            tags: tags.into(),
        })
    }

    /// Returns the content identity of the complete profile.
    #[must_use]
    pub const fn id(&self) -> IccProfileId {
        self.id
    }

    /// Returns the exact validated ICC bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the declared ICC profile-format version.
    #[must_use]
    pub const fn version(&self) -> IccVersion {
        self.version
    }

    /// Returns the validated profile/device class.
    #[must_use]
    pub const fn class(&self) -> IccProfileClass {
        self.class
    }

    /// Returns the validated device-side color space.
    #[must_use]
    pub const fn data_color_space(&self) -> IccColorSpace {
        self.data_color_space
    }

    /// Returns the validated profile connection space.
    #[must_use]
    pub const fn connection_space(&self) -> IccColorSpace {
        self.connection_space
    }

    /// Returns the profile header's rendering intent.
    #[must_use]
    pub const fn rendering_intent(&self) -> IccRenderingIntent {
        self.rendering_intent
    }

    /// Returns the complete transform model required by this display profile.
    #[must_use]
    pub const fn display_model(&self) -> IccDisplayModel {
        self.display_model
    }

    /// Returns the validated tag directory in source order.
    #[must_use]
    pub fn tags(&self) -> &[IccTag] {
        &self.tags
    }
}

/// Stable monitor identity supplied by one platform discovery lifetime.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MonitorId(String);

impl MonitorId {
    /// Creates a non-empty bounded platform monitor identifier.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
            return Err(invalid(
                "create_monitor_id",
                "monitor identity must be non-empty, bounded, and free of control characters",
            )
            .with_context(
                profile_context("create_monitor_id").with_field("length", value.len().to_string()),
            ));
        }
        Ok(Self(value))
    }

    /// Returns the platform identity string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MonitorId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One raw platform display observation from a discovery pass.
///
/// An absent ICC buffer is retained explicitly. The engine never guesses sRGB
/// or reuses another monitor's profile when the platform exports no profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplayProfileObservation {
    id: MonitorId,
    name: String,
    primary: bool,
    built_in: bool,
    icc_profile_bytes: Option<Arc<[u8]>>,
}

impl DisplayProfileObservation {
    /// Creates a bounded platform display observation.
    pub fn new(
        id: MonitorId,
        name: impl Into<String>,
        primary: bool,
        built_in: bool,
        icc_profile_bytes: Option<Arc<[u8]>>,
    ) -> Result<Self> {
        let name = name.into();
        if name.is_empty() || name.len() > 256 || name.chars().any(char::is_control) {
            return Err(invalid(
                "create_display_observation",
                "display name must be non-empty, bounded, and free of control characters",
            )
            .with_context(display_context("create_display_observation", &id)));
        }
        if let Some(bytes) = icc_profile_bytes.as_ref() {
            validate_profile_length(bytes.len())?;
        }
        Ok(Self {
            id,
            name,
            primary,
            built_in,
            icc_profile_bytes,
        })
    }

    /// Returns the stable monitor identity.
    #[must_use]
    pub const fn id(&self) -> &MonitorId {
        &self.id
    }

    /// Returns the human-readable platform display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Reports whether the platform identifies this as its primary display.
    #[must_use]
    pub const fn is_primary(&self) -> bool {
        self.primary
    }

    /// Reports whether the display is integrated into the computer.
    #[must_use]
    pub const fn is_built_in(&self) -> bool {
        self.built_in
    }

    /// Returns exact platform ICC bytes, or `None` for explicit unprofiled state.
    #[must_use]
    pub fn icc_profile_bytes(&self) -> Option<&[u8]> {
        self.icc_profile_bytes.as_deref()
    }
}

/// Platform boundary for one complete active-display discovery pass.
pub trait DisplayProfileDiscovery {
    /// Returns every active display as one atomic observation set.
    fn discover(&self) -> Result<Vec<DisplayProfileObservation>>;
}

/// Complete native display snapshot supplied by the desktop shell.
///
/// Windows and Linux shells already own the window-system connection that
/// identifies the monitor containing a viewport. They submit the exact IDs and
/// profile bytes from that same connection through this provider, avoiding a
/// second X11 or Wayland connection with incompatible output identities. Use
/// the Win32 display device name, the X11 RandR CRTC ID, or the Wayland output
/// ID from the shell connection as [`MonitorId`]. Missing platform profile data
/// remains an explicit unprofiled observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeDisplayProfileProvider {
    observations: Arc<[DisplayProfileObservation]>,
}

impl NativeDisplayProfileProvider {
    /// Captures one bounded, atomic native display-event snapshot.
    pub fn new(observations: Vec<DisplayProfileObservation>) -> Result<Self> {
        if observations.len() > MAX_ACTIVE_DISPLAYS {
            return Err(resource_exhausted(
                "create_native_display_provider",
                "native display snapshot exceeds the fixed display limit",
            )
            .with_context(
                profile_context("create_native_display_provider")
                    .with_field("display_count", observations.len().to_string())
                    .with_field("display_limit", MAX_ACTIVE_DISPLAYS.to_string()),
            ));
        }
        Ok(Self {
            observations: observations.into(),
        })
    }

    /// Returns the exact shell-owned observation set.
    #[must_use]
    pub fn observations(&self) -> &[DisplayProfileObservation] {
        &self.observations
    }
}

impl DisplayProfileDiscovery for NativeDisplayProfileProvider {
    fn discover(&self) -> Result<Vec<DisplayProfileObservation>> {
        Ok(self.observations.to_vec())
    }
}

/// Direct CoreGraphics active-display profile discovery on macOS.
///
/// Windows and Linux shells use [`NativeDisplayProfileProvider`] so monitor
/// identity comes from the same window-system connection that owns the native
/// viewport. This direct system provider is deliberately available only where
/// the repository owns a complete target-native discovery implementation.
#[derive(Clone, Copy, Debug, Default)]
#[cfg(target_os = "macos")]
pub struct SystemDisplayProfileDiscovery;

#[cfg(target_os = "macos")]
impl DisplayProfileDiscovery for SystemDisplayProfileDiscovery {
    fn discover(&self) -> Result<Vec<DisplayProfileObservation>> {
        macos::discover()
    }
}

/// One validated active display and its optional current ICC profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplayProfile {
    id: MonitorId,
    name: String,
    primary: bool,
    built_in: bool,
    profile: Option<IccProfile>,
}

impl DisplayProfile {
    /// Returns the monitor identity.
    #[must_use]
    pub const fn id(&self) -> &MonitorId {
        &self.id
    }

    /// Returns the platform display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Reports whether this is the current primary display.
    #[must_use]
    pub const fn is_primary(&self) -> bool {
        self.primary
    }

    /// Reports whether this display is integrated into the computer.
    #[must_use]
    pub const fn is_built_in(&self) -> bool {
        self.built_in
    }

    /// Returns the current validated ICC profile, if the platform exported one.
    #[must_use]
    pub const fn profile(&self) -> Option<&IccProfile> {
        self.profile.as_ref()
    }
}

/// Immutable active-display state published to engine consumers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplayProfileSnapshot {
    generation: u64,
    displays: Arc<[DisplayProfile]>,
}

impl Default for DisplayProfileSnapshot {
    fn default() -> Self {
        Self {
            generation: 0,
            displays: Arc::from([]),
        }
    }
}

impl DisplayProfileSnapshot {
    /// Returns the generation of the last semantic display-state change.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns active displays in deterministic monitor identity order.
    #[must_use]
    pub fn displays(&self) -> &[DisplayProfile] {
        &self.displays
    }

    /// Returns the exact active display with this identity.
    #[must_use]
    pub fn display(&self, id: &MonitorId) -> Option<&DisplayProfile> {
        self.displays
            .binary_search_by(|display| display.id.cmp(id))
            .ok()
            .map(|index| &self.displays[index])
    }

    /// Returns the active primary display when one exists.
    #[must_use]
    pub fn primary_display(&self) -> Option<&DisplayProfile> {
        self.displays.iter().find(|display| display.primary)
    }

    /// Binds one viewport presentation to an exact monitor in this snapshot.
    pub fn bind_for_presentation(
        &self,
        monitor_id: &MonitorId,
    ) -> Result<MonitorPresentationBinding> {
        let display = self.display(monitor_id).ok_or_else(|| {
            Error::new(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "presentation monitor is not active",
            )
            .with_context(display_context("bind_monitor_presentation", monitor_id))
        })?;
        let state = display
            .profile
            .clone()
            .map_or(PresentationProfileState::Unprofiled, |profile| {
                PresentationProfileState::Profiled { profile }
            });
        Ok(MonitorPresentationBinding {
            catalog_generation: self.generation,
            monitor_id: monitor_id.clone(),
            state,
        })
    }
}

/// Exact display profile state captured by one presentation binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PresentationProfileState {
    /// Presentation must use this exact validated profile artifact.
    Profiled {
        /// Profile retained for transform compilation and inspection.
        profile: IccProfile,
    },
    /// The platform exported no profile for this monitor.
    Unprofiled,
}

impl PresentationProfileState {
    /// Returns the profile when presentation is explicitly profiled.
    #[must_use]
    pub const fn profile(&self) -> Option<&IccProfile> {
        match self {
            Self::Profiled { profile } => Some(profile),
            Self::Unprofiled => None,
        }
    }
}

/// Monitor and profile selection used to compile one viewport output transform.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonitorPresentationBinding {
    catalog_generation: u64,
    monitor_id: MonitorId,
    state: PresentationProfileState,
}

impl MonitorPresentationBinding {
    /// Returns the catalog generation observed while binding.
    #[must_use]
    pub const fn catalog_generation(&self) -> u64 {
        self.catalog_generation
    }

    /// Returns the target monitor identity.
    #[must_use]
    pub const fn monitor_id(&self) -> &MonitorId {
        &self.monitor_id
    }

    /// Returns explicit profiled or unprofiled presentation state.
    #[must_use]
    pub const fn state(&self) -> &PresentationProfileState {
        &self.state
    }

    /// Returns the selected profile identity when one exists.
    #[must_use]
    pub fn profile_id(&self) -> Option<IccProfileId> {
        self.state.profile().map(IccProfile::id)
    }

    /// Reports whether the same monitor still exposes the same profile state.
    #[must_use]
    pub fn is_current(&self, snapshot: &DisplayProfileSnapshot) -> bool {
        self.catalog_generation == snapshot.generation
            && snapshot.display(&self.monitor_id).is_some_and(|display| {
                display.profile.as_ref().map(IccProfile::id) == self.profile_id()
            })
    }
}

/// Semantic changes produced by one successful atomic catalog refresh.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplayProfileUpdate {
    generation: u64,
    changed: bool,
    added: Vec<MonitorId>,
    removed: Vec<MonitorId>,
    profile_changed: Vec<MonitorId>,
}

impl DisplayProfileUpdate {
    /// Returns the resulting catalog generation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Reports whether any display state changed.
    #[must_use]
    pub const fn changed(&self) -> bool {
        self.changed
    }

    /// Returns newly active monitors in deterministic order.
    #[must_use]
    pub fn added(&self) -> &[MonitorId] {
        &self.added
    }

    /// Returns monitors no longer active in deterministic order.
    #[must_use]
    pub fn removed(&self) -> &[MonitorId] {
        &self.removed
    }

    /// Returns monitors whose profiled or unprofiled identity changed.
    #[must_use]
    pub fn profile_changed(&self) -> &[MonitorId] {
        &self.profile_changed
    }
}

/// Single-owner active-display profile registry.
///
/// Discovery and parsing finish into temporary state before the published
/// snapshot changes. A malformed profile, duplicate monitor, or invalid
/// primary-display set therefore leaves the prior snapshot intact.
#[derive(Debug, Default)]
pub struct DisplayProfileCatalog {
    snapshot: DisplayProfileSnapshot,
}

impl DisplayProfileCatalog {
    /// Creates an empty catalog at generation zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns an immutable clone of current engine-visible display state.
    #[must_use]
    pub fn snapshot(&self) -> DisplayProfileSnapshot {
        self.snapshot.clone()
    }

    /// Atomically discovers, validates, sorts, and publishes active displays.
    pub fn refresh(
        &mut self,
        discovery: &impl DisplayProfileDiscovery,
    ) -> Result<DisplayProfileUpdate> {
        let observations = discovery.discover()?;
        if observations.len() > MAX_ACTIVE_DISPLAYS {
            return Err(resource_exhausted(
                "refresh_display_profiles",
                "active display count exceeds the fixed discovery limit",
            )
            .with_context(
                profile_context("refresh_display_profiles")
                    .with_field("display_count", observations.len().to_string())
                    .with_field("display_limit", MAX_ACTIVE_DISPLAYS.to_string()),
            ));
        }

        let mut displays = observations
            .into_iter()
            .map(validate_observation)
            .collect::<Result<Vec<_>>>()?;
        displays.sort_by(|left, right| left.id.cmp(&right.id));
        validate_display_set(&displays)?;

        let (added, removed, profile_changed) = diff_displays(&self.snapshot.displays, &displays);
        let changed = self.snapshot.displays.as_ref() != displays.as_slice();
        let generation = if changed {
            self.snapshot.generation.checked_add(1).ok_or_else(|| {
                Error::new(
                    ErrorCategory::Internal,
                    Recoverability::Terminal,
                    "display profile generation is exhausted",
                )
                .with_context(profile_context("refresh_display_profiles"))
            })?
        } else {
            self.snapshot.generation
        };

        if changed {
            self.snapshot = DisplayProfileSnapshot {
                generation,
                displays: displays.into(),
            };
        }
        Ok(DisplayProfileUpdate {
            generation,
            changed,
            added,
            removed,
            profile_changed,
        })
    }

    /// Binds one viewport presentation to an exact current monitor profile.
    pub fn bind_for_presentation(
        &self,
        monitor_id: &MonitorId,
    ) -> Result<MonitorPresentationBinding> {
        self.snapshot.bind_for_presentation(monitor_id)
    }
}

fn validate_profile_length(length: usize) -> Result<()> {
    let minimum = ICC_HEADER_BYTES + ICC_TAG_COUNT_BYTES;
    if length < minimum {
        return Err(corrupt(
            "validate_profile_length",
            "ICC profile is shorter than its header and tag count",
        )
        .with_context(
            profile_context("validate_profile_length")
                .with_field("length", length.to_string())
                .with_field("minimum", minimum.to_string()),
        ));
    }
    if length > MAX_ICC_PROFILE_BYTES {
        return Err(resource_exhausted(
            "validate_profile_length",
            "ICC profile exceeds the fixed byte limit",
        )
        .with_context(
            profile_context("validate_profile_length")
                .with_field("length", length.to_string())
                .with_field("limit", MAX_ICC_PROFILE_BYTES.to_string()),
        ));
    }
    if length % 4 != 0 {
        return Err(corrupt(
            "validate_profile_length",
            "ICC profile length must include zero padding to a four-byte boundary",
        )
        .with_context(
            profile_context("validate_profile_length").with_field("length", length.to_string()),
        ));
    }
    Ok(())
}

fn validate_header(bytes: &[u8]) -> Result<()> {
    const D50_ILLUMINANT: [u8; 12] = [
        0x00, 0x00, 0xf6, 0xd6, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0xd3, 0x2d,
    ];
    if bytes[68..80] != D50_ILLUMINANT {
        return Err(corrupt(
            "validate_profile_header",
            "ICC profile connection-space illuminant must be the encoded D50 adopted white",
        ));
    }
    if bytes[100..128].iter().any(|byte| *byte != 0) {
        return Err(corrupt(
            "validate_profile_header",
            "ICC profile reserved header bytes must be zero",
        ));
    }
    Ok(())
}

fn parse_version(bytes: &[u8]) -> Result<IccVersion> {
    let major = bytes[8];
    let minor = bytes[9] >> 4;
    let bugfix = bytes[9] & 0x0f;
    if !matches!(major, 2 | 4) || minor > 9 || bugfix > 9 || bytes[10..12] != [0, 0] {
        return Err(unsupported(
            "validate_profile_version",
            "ICC display profile must use a structurally valid v2 or v4 version",
        )
        .with_context(
            profile_context("validate_profile_version")
                .with_field("major", major.to_string())
                .with_field("minor", minor.to_string())
                .with_field("bugfix", bugfix.to_string()),
        ));
    }
    Ok(IccVersion {
        major,
        minor,
        bugfix,
    })
}

fn parse_profile_class(signature: &[u8]) -> Result<IccProfileClass> {
    if signature == b"mntr" {
        Ok(IccProfileClass::Display)
    } else {
        Err(unsupported(
            "validate_profile_class",
            "ICC profile is not a display device profile",
        )
        .with_context(signature_context("validate_profile_class", signature)))
    }
}

fn parse_data_color_space(signature: &[u8]) -> Result<IccColorSpace> {
    if signature == b"RGB " {
        Ok(IccColorSpace::Rgb)
    } else {
        Err(unsupported(
            "validate_data_color_space",
            "display profile device space must be RGB",
        )
        .with_context(signature_context("validate_data_color_space", signature)))
    }
}

fn parse_connection_space(signature: &[u8]) -> Result<IccColorSpace> {
    match signature {
        b"XYZ " => Ok(IccColorSpace::Xyz),
        b"Lab " => Ok(IccColorSpace::Lab),
        _ => Err(unsupported(
            "validate_connection_space",
            "display profile connection space must be XYZ or Lab",
        )
        .with_context(signature_context("validate_connection_space", signature))),
    }
}

fn parse_rendering_intent(value: u32) -> Result<IccRenderingIntent> {
    match value {
        0 => Ok(IccRenderingIntent::Perceptual),
        1 => Ok(IccRenderingIntent::MediaRelativeColorimetric),
        2 => Ok(IccRenderingIntent::Saturation),
        3 => Ok(IccRenderingIntent::AbsoluteColorimetric),
        _ => Err(corrupt(
            "validate_rendering_intent",
            "ICC profile declares an unknown rendering intent",
        )
        .with_context(
            profile_context("validate_rendering_intent").with_field("intent", value.to_string()),
        )),
    }
}

fn parse_tags(bytes: Arc<[u8]>) -> Result<Vec<IccTag>> {
    let tag_count = read_u32(&bytes, ICC_HEADER_BYTES, "read_tag_count")? as usize;
    if tag_count > MAX_ICC_TAGS {
        return Err(resource_exhausted(
            "validate_tag_count",
            "ICC tag count exceeds the fixed limit",
        )
        .with_context(
            profile_context("validate_tag_count")
                .with_field("tag_count", tag_count.to_string())
                .with_field("tag_limit", MAX_ICC_TAGS.to_string()),
        ));
    }
    let table_bytes = tag_count
        .checked_mul(ICC_TAG_ENTRY_BYTES)
        .and_then(|size| size.checked_add(ICC_HEADER_BYTES + ICC_TAG_COUNT_BYTES))
        .ok_or_else(|| corrupt("validate_tag_table", "ICC tag table size overflows"))?;
    if table_bytes > bytes.len() {
        return Err(corrupt(
            "validate_tag_table",
            "ICC tag table extends beyond the profile",
        ));
    }

    let mut signatures = BTreeSet::new();
    let mut elements = BTreeMap::<usize, usize>::new();
    let mut tags = Vec::with_capacity(tag_count);
    for index in 0..tag_count {
        let entry = ICC_HEADER_BYTES + ICC_TAG_COUNT_BYTES + index * ICC_TAG_ENTRY_BYTES;
        let signature: [u8; 4] = bytes[entry..entry + 4]
            .try_into()
            .expect("ICC tag signature slice has fixed length");
        if !signatures.insert(signature) {
            return Err(corrupt(
                "validate_tag_signature",
                "ICC tag directory contains a duplicate signature",
            )
            .with_context(
                signature_context("validate_tag_signature", &signature)
                    .with_field("tag_index", index.to_string()),
            ));
        }
        let offset = read_u32(&bytes, entry + 4, "read_tag_offset")? as usize;
        let size = read_u32(&bytes, entry + 8, "read_tag_size")? as usize;
        let end = offset
            .checked_add(size)
            .ok_or_else(|| corrupt("validate_tag_range", "ICC tag range overflows"))?;
        if offset < table_bytes || offset % 4 != 0 || size < 8 || end > bytes.len() {
            return Err(corrupt(
                "validate_tag_range",
                "ICC tag range is empty, unaligned, overlaps the directory, or exceeds the profile",
            )
            .with_context(
                signature_context("validate_tag_range", &signature)
                    .with_field("tag_index", index.to_string())
                    .with_field("offset", offset.to_string())
                    .with_field("size", size.to_string())
                    .with_field("profile_size", bytes.len().to_string()),
            ));
        }
        if bytes[offset + 4..offset + 8].iter().any(|byte| *byte != 0) {
            return Err(corrupt(
                "validate_tag_type",
                "ICC tag data type reserved bytes must be zero",
            )
            .with_context(
                signature_context("validate_tag_type", &signature)
                    .with_field("tag_index", index.to_string()),
            ));
        }
        if let Some(existing_size) = elements.insert(offset, size) {
            if existing_size != size {
                return Err(corrupt(
                    "validate_tag_range",
                    "shared ICC tag elements must use the exact same offset and size",
                )
                .with_context(
                    signature_context("validate_tag_range", &signature)
                        .with_field("tag_index", index.to_string()),
                ));
            }
        }
        tags.push(IccTag {
            signature,
            profile_bytes: Arc::clone(&bytes),
            offset,
            size,
        });
    }
    validate_tag_element_layout(&bytes, table_bytes, &elements)?;
    Ok(tags)
}

fn validate_tag_element_layout(
    bytes: &[u8],
    table_bytes: usize,
    elements: &BTreeMap<usize, usize>,
) -> Result<()> {
    let mut expected_offset = table_bytes;
    for (&offset, &size) in elements {
        if offset != expected_offset {
            let message = if offset < expected_offset {
                "ICC tag data elements partially overlap"
            } else {
                "ICC tag data elements are not contiguous after the tag table"
            };
            return Err(corrupt("validate_tag_layout", message).with_context(
                profile_context("validate_tag_layout")
                    .with_field("expected_offset", expected_offset.to_string())
                    .with_field("actual_offset", offset.to_string()),
            ));
        }
        let end = offset
            .checked_add(size)
            .ok_or_else(|| corrupt("validate_tag_layout", "ICC tag data range overflows"))?;
        let padded_end = end
            .checked_add(3)
            .map(|value| value & !3)
            .ok_or_else(|| corrupt("validate_tag_layout", "ICC tag padding range overflows"))?;
        if padded_end > bytes.len() {
            return Err(corrupt(
                "validate_tag_layout",
                "ICC tag data padding extends beyond the profile",
            ));
        }
        if bytes[end..padded_end].iter().any(|byte| *byte != 0) {
            return Err(corrupt(
                "validate_tag_padding",
                "ICC tag data padding bytes must be zero",
            ));
        }
        expected_offset = padded_end;
    }
    if expected_offset != bytes.len() {
        return Err(corrupt(
            "validate_tag_layout",
            "ICC profile has bytes outside its contiguous tag data elements",
        )
        .with_context(
            profile_context("validate_tag_layout")
                .with_field("tag_data_end", expected_offset.to_string())
                .with_field("profile_size", bytes.len().to_string()),
        ));
    }
    Ok(())
}

fn validate_required_display_tags(
    tags: &[IccTag],
    connection_space: IccColorSpace,
) -> Result<IccDisplayModel> {
    validate_required_tag(tags, *b"desc", &[*b"mluc", *b"desc"])?;
    validate_required_tag(tags, *b"cprt", &[*b"mluc", *b"text"])?;
    validate_required_tag(tags, *b"wtpt", &[*b"XYZ "])?;

    const MATRIX_TAGS: [([u8; 4], [[u8; 4]; 2]); 6] = [
        (*b"rXYZ", [*b"XYZ ", *b"XYZ "]),
        (*b"gXYZ", [*b"XYZ ", *b"XYZ "]),
        (*b"bXYZ", [*b"XYZ ", *b"XYZ "]),
        (*b"rTRC", [*b"curv", *b"para"]),
        (*b"gTRC", [*b"curv", *b"para"]),
        (*b"bTRC", [*b"curv", *b"para"]),
    ];
    let matrix_count = MATRIX_TAGS
        .iter()
        .filter(|(signature, _)| find_tag(tags, *signature).is_some())
        .count();
    if matrix_count == MATRIX_TAGS.len() {
        if connection_space != IccColorSpace::Xyz {
            return Err(corrupt(
                "validate_display_model",
                "matrix/TRC display profiles require the XYZ connection space",
            ));
        }
        for (signature, types) in MATRIX_TAGS {
            validate_required_tag(tags, signature, &types)?;
        }
        return Ok(IccDisplayModel::MatrixTrc);
    }

    let a_to_b = find_tag(tags, *b"A2B0");
    let b_to_a = find_tag(tags, *b"B2A0");
    if a_to_b.is_some() && b_to_a.is_some() {
        validate_required_tag(tags, *b"A2B0", &[*b"mft1", *b"mft2", *b"mAB "])?;
        validate_required_tag(tags, *b"B2A0", &[*b"mft1", *b"mft2", *b"mBA "])?;
        return Ok(IccDisplayModel::Lut);
    }

    Err(corrupt(
        "validate_display_model",
        "RGB display profile must contain a complete matrix/TRC or paired LUT transform model",
    )
    .with_context(
        profile_context("validate_display_model")
            .with_field("matrix_tag_count", matrix_count.to_string())
            .with_field("has_a_to_b_0", a_to_b.is_some().to_string())
            .with_field("has_b_to_a_0", b_to_a.is_some().to_string()),
    ))
}

fn validate_required_tag(
    tags: &[IccTag],
    signature: [u8; 4],
    permitted_types: &[[u8; 4]],
) -> Result<()> {
    let tag = find_tag(tags, signature).ok_or_else(|| {
        corrupt(
            "validate_required_tag",
            "ICC display profile is missing a required tag",
        )
        .with_context(signature_context("validate_required_tag", &signature))
    })?;
    let data_type = tag.data_type_signature();
    if !permitted_types.contains(&data_type) {
        return Err(corrupt(
            "validate_required_tag_type",
            "ICC required tag uses a data type that is not permitted for that tag",
        )
        .with_context(
            signature_context("validate_required_tag_type", &signature)
                .with_field("data_type", hex_bytes(&data_type)),
        ));
    }
    Ok(())
}

fn find_tag(tags: &[IccTag], signature: [u8; 4]) -> Option<&IccTag> {
    tags.iter().find(|tag| tag.signature == signature)
}

fn validate_observation(observation: DisplayProfileObservation) -> Result<DisplayProfile> {
    let profile = observation
        .icc_profile_bytes
        .map(IccProfile::parse)
        .transpose()
        .map_err(|error| {
            error.with_context(display_context(
                "validate_discovered_profile",
                &observation.id,
            ))
        })?;
    Ok(DisplayProfile {
        id: observation.id,
        name: observation.name,
        primary: observation.primary,
        built_in: observation.built_in,
        profile,
    })
}

fn validate_display_set(displays: &[DisplayProfile]) -> Result<()> {
    for pair in displays.windows(2) {
        if pair[0].id == pair[1].id {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::Retryable,
                "display discovery returned a duplicate monitor identity",
            )
            .with_context(display_context("validate_display_set", &pair[0].id)));
        }
    }
    let primary_count = displays.iter().filter(|display| display.primary).count();
    if primary_count > 1 {
        return Err(Error::new(
            ErrorCategory::Conflict,
            Recoverability::Retryable,
            "display discovery returned more than one primary display",
        )
        .with_context(
            profile_context("validate_display_set")
                .with_field("primary_count", primary_count.to_string()),
        ));
    }
    Ok(())
}

fn diff_displays(
    before: &[DisplayProfile],
    after: &[DisplayProfile],
) -> (Vec<MonitorId>, Vec<MonitorId>, Vec<MonitorId>) {
    let before_ids = before
        .iter()
        .map(|display| &display.id)
        .collect::<BTreeSet<_>>();
    let after_ids = after
        .iter()
        .map(|display| &display.id)
        .collect::<BTreeSet<_>>();
    let added = after_ids
        .difference(&before_ids)
        .map(|id| (*id).clone())
        .collect();
    let removed = before_ids
        .difference(&after_ids)
        .map(|id| (*id).clone())
        .collect();
    let profile_changed = before
        .iter()
        .filter_map(|old| {
            after
                .binary_search_by(|new| new.id.cmp(&old.id))
                .ok()
                .and_then(|index| {
                    let new = &after[index];
                    (old.profile.as_ref().map(IccProfile::id)
                        != new.profile.as_ref().map(IccProfile::id))
                    .then(|| old.id.clone())
                })
        })
        .collect();
    (added, removed, profile_changed)
}

fn read_u32(bytes: &[u8], offset: usize, operation: &'static str) -> Result<u32> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| corrupt(operation, "ICC integer offset overflows"))?;
    let value = bytes
        .get(offset..end)
        .ok_or_else(|| corrupt(operation, "ICC integer extends beyond the profile"))?;
    Ok(u32::from_be_bytes(
        value
            .try_into()
            .expect("validated ICC integer has four bytes"),
    ))
}

fn signature_context(operation: &'static str, signature: &[u8]) -> ErrorContext {
    profile_context(operation).with_field("signature", hex_bytes(signature))
}

fn display_context(operation: &'static str, id: &MonitorId) -> ErrorContext {
    profile_context(operation).with_field("monitor_id", id.as_str())
}

fn profile_context(operation: &'static str) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation)
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use fmt::Write as _;
        write!(encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(profile_context(operation))
}

fn corrupt(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(profile_context(operation))
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(profile_context(operation))
}

fn resource_exhausted(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(profile_context(operation))
}
