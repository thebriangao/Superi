//! Storage-neutral channel and nested layer names for multilayer images.
//!
//! Channel names follow the OpenEXR-compatible `layer.channel` convention when
//! every period-delimited component is non-empty. Other valid channel strings
//! remain unqualified custom names. Values preserve exact Unicode spelling and
//! case. This module does not infer storage layout, alpha association, stacking,
//! or compositing behavior from a name.

use std::collections::BTreeSet;
use std::fmt;
use std::ops::Index;
use std::str::FromStr;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

/// The stable position of a channel in an ordered [`ChannelList`].
///
/// Storage implementations can retain this value while changing between
/// packed, planar, tiled, or region views. The index has meaning only together
/// with the channel list that produced it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChannelIndex(usize);

impl ChannelIndex {
    /// Creates an index from its zero-based position.
    #[must_use]
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    /// Returns the zero-based position.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

impl From<usize> for ChannelIndex {
    fn from(index: usize) -> Self {
        Self::new(index)
    }
}

impl From<ChannelIndex> for usize {
    fn from(index: ChannelIndex) -> Self {
        index.get()
    }
}

/// A canonical nested image-layer path.
///
/// The empty path is the base layer. Non-base paths contain one or more
/// non-empty period-delimited components. Names are case-sensitive and are not
/// normalized, so a file backend can round-trip exact identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LayerName(String);

impl LayerName {
    /// Returns the unnamed base layer that directly contains unqualified channels.
    #[must_use]
    pub fn base() -> Self {
        Self(String::new())
    }

    /// Constructs a layer path from ordered components.
    ///
    /// An empty iterator produces the base layer.
    pub fn from_components<I, S>(components: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let components = components
            .into_iter()
            .map(|component| component.as_ref().to_owned())
            .collect::<Vec<_>>();
        for component in &components {
            validate_component(component, "create_layer_name", "layer component")?;
        }
        Ok(Self(components.join(".")))
    }

    /// Returns the exact dot-delimited path, or an empty string for the base layer.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns true for the unnamed base layer.
    #[must_use]
    pub fn is_base(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns this path's components from outermost to innermost.
    pub fn components(&self) -> impl DoubleEndedIterator<Item = &str> {
        self.0.split('.').filter(|component| !component.is_empty())
    }

    /// Returns the number of hierarchy components. The base layer has depth zero.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.components().count()
    }

    /// Returns the directly enclosing layer.
    ///
    /// A top-level layer's parent is the base layer. The base layer has no parent.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        if self.is_base() {
            None
        } else if let Some((parent, _)) = self.0.rsplit_once('.') {
            Some(Self(parent.to_owned()))
        } else {
            Some(Self::base())
        }
    }

    /// Appends one validated child component.
    pub fn child(&self, component: impl AsRef<str>) -> Result<Self> {
        let component = component.as_ref();
        validate_component(component, "create_child_layer", "layer component")?;
        if self.is_base() {
            Ok(Self(component.to_owned()))
        } else {
            Ok(Self(format!("{}.{}", self.0, component)))
        }
    }

    /// Returns true when this layer is a strict ancestor of `other`.
    #[must_use]
    pub fn encloses(&self, other: &Self) -> bool {
        if self == other {
            return false;
        }
        if self.is_base() {
            return true;
        }
        other
            .0
            .strip_prefix(self.as_str())
            .is_some_and(|remainder| remainder.starts_with('.'))
    }

    /// Returns true when this layer is the immediate parent of `other`.
    #[must_use]
    pub fn directly_encloses(&self, other: &Self) -> bool {
        other.parent().as_ref() == Some(self)
    }

    fn prefixes(&self) -> impl Iterator<Item = Self> + '_ {
        let mut prefix = String::new();
        self.components().map(move |component| {
            if !prefix.is_empty() {
                prefix.push('.');
            }
            prefix.push_str(component);
            Self(prefix.clone())
        })
    }
}

impl Default for LayerName {
    fn default() -> Self {
        Self::base()
    }
}

impl fmt::Display for LayerName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for LayerName {
    type Err = Error;

    fn from_str(name: &str) -> Result<Self> {
        if name.is_empty() {
            return Ok(Self::base());
        }
        validate_qualified(name, "parse_layer_name", "layer name")?;
        Ok(Self(name.to_owned()))
    }
}

impl TryFrom<&str> for LayerName {
    type Error = Error;

    fn try_from(name: &str) -> Result<Self> {
        Self::from_str(name)
    }
}

impl TryFrom<String> for LayerName {
    type Error = Error;

    fn try_from(name: String) -> Result<Self> {
        Self::from_str(&name)
    }
}

/// Conventional meanings recognized from a channel's base name.
///
/// Recognition is advisory and case-sensitive. It does not change the exact
/// channel name, alpha mode, or any storage property. Unknown names remain fully
/// supported and return `None` from [`ChannelName::standard`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum StandardChannel {
    /// Red color intensity, `R`.
    Red,
    /// Green color intensity, `G`.
    Green,
    /// Blue color intensity, `B`.
    Blue,
    /// Alpha or opacity, `A`.
    Alpha,
    /// Luminance, `Y`.
    Luminance,
    /// Red chroma difference, `RY`.
    RedChroma,
    /// Blue chroma difference, `BY`.
    BlueChroma,
    /// Red opacity, `AR`.
    RedAlpha,
    /// Green opacity, `AG`.
    GreenAlpha,
    /// Blue opacity, `AB`.
    BlueAlpha,
    /// Front depth, `Z`.
    Depth,
    /// Back depth, `ZBack`.
    BackDepth,
    /// Object identifier, `id`.
    ObjectId,
}

impl StandardChannel {
    /// Every conventional channel recognized by this version.
    pub const ALL: &'static [Self] = &[
        Self::Red,
        Self::Green,
        Self::Blue,
        Self::Alpha,
        Self::Luminance,
        Self::RedChroma,
        Self::BlueChroma,
        Self::RedAlpha,
        Self::GreenAlpha,
        Self::BlueAlpha,
        Self::Depth,
        Self::BackDepth,
        Self::ObjectId,
    ];

    /// Returns the canonical, case-sensitive base name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Red => "R",
            Self::Green => "G",
            Self::Blue => "B",
            Self::Alpha => "A",
            Self::Luminance => "Y",
            Self::RedChroma => "RY",
            Self::BlueChroma => "BY",
            Self::RedAlpha => "AR",
            Self::GreenAlpha => "AG",
            Self::BlueAlpha => "AB",
            Self::Depth => "Z",
            Self::BackDepth => "ZBack",
            Self::ObjectId => "id",
        }
    }

    /// Recognizes one exact conventional base name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "R" => Some(Self::Red),
            "G" => Some(Self::Green),
            "B" => Some(Self::Blue),
            "A" => Some(Self::Alpha),
            "Y" => Some(Self::Luminance),
            "RY" => Some(Self::RedChroma),
            "BY" => Some(Self::BlueChroma),
            "AR" => Some(Self::RedAlpha),
            "AG" => Some(Self::GreenAlpha),
            "AB" => Some(Self::BlueAlpha),
            "Z" => Some(Self::Depth),
            "ZBack" => Some(Self::BackDepth),
            "id" => Some(Self::ObjectId),
            _ => None,
        }
    }
}

impl fmt::Display for StandardChannel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// The exact qualified name of one image channel.
///
/// For a conventionally qualified name, the final component is the channel's
/// base name and preceding components form its nested [`LayerName`]. Names with
/// leading, trailing, or consecutive periods remain exact unqualified custom
/// names instead of being rejected or silently rewritten.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChannelName {
    full: String,
    layer: LayerName,
    base: String,
}

impl ChannelName {
    /// Creates an unqualified channel in the base layer.
    pub fn new(base_name: impl AsRef<str>) -> Result<Self> {
        let base_name = base_name.as_ref();
        validate_component(base_name, "create_channel_name", "channel name")?;
        Ok(Self {
            full: base_name.to_owned(),
            layer: LayerName::base(),
            base: base_name.to_owned(),
        })
    }

    /// Creates a channel in an explicit layer.
    pub fn in_layer(layer: LayerName, base_name: impl AsRef<str>) -> Result<Self> {
        let base_name = base_name.as_ref();
        validate_component(base_name, "create_channel_name", "channel name")?;
        let full = if layer.is_base() {
            base_name.to_owned()
        } else {
            format!("{layer}.{base_name}")
        };
        Ok(Self {
            full,
            layer,
            base: base_name.to_owned(),
        })
    }

    /// Returns the exact qualified name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.full
    }

    /// Returns the exact final channel component.
    #[must_use]
    pub fn base_name(&self) -> &str {
        &self.base
    }

    /// Returns this channel's layer, including the base layer when unqualified.
    #[must_use]
    pub fn layer(&self) -> &LayerName {
        &self.layer
    }

    /// Recognizes the conventional meaning of this channel's base name.
    ///
    /// The result does not imply alpha association, stacking, or compositing
    /// behavior.
    #[must_use]
    pub fn standard(&self) -> Option<StandardChannel> {
        StandardChannel::from_name(self.base_name())
    }
}

impl fmt::Display for ChannelName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ChannelName {
    type Err = Error;

    fn from_str(name: &str) -> Result<Self> {
        validate_unqualified(name, "parse_channel_name", "channel name")?;
        let is_qualified =
            name.contains('.') && name.split('.').all(|component| !component.is_empty());
        if is_qualified {
            let (layer, base) = name
                .rsplit_once('.')
                .expect("a qualified name contains a period");
            Ok(Self {
                full: name.to_owned(),
                layer: LayerName(layer.to_owned()),
                base: base.to_owned(),
            })
        } else {
            Ok(Self {
                full: name.to_owned(),
                layer: LayerName::base(),
                base: name.to_owned(),
            })
        }
    }
}

impl TryFrom<&str> for ChannelName {
    type Error = Error;

    fn try_from(name: &str) -> Result<Self> {
        Self::from_str(name)
    }
}

impl TryFrom<String> for ChannelName {
    type Error = Error;

    fn try_from(name: String) -> Result<Self> {
        Self::from_str(&name)
    }
}

/// A non-empty, ordered set of uniquely named image channels.
///
/// Ordering is image layout identity, but no packed, planar, tiled, or numeric
/// storage is implied. Derived layers are ordered by first channel appearance,
/// with the base layer first and ancestors before descendants.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ChannelList {
    channels: Vec<ChannelName>,
    layers: Vec<LayerName>,
}

impl ChannelList {
    /// Creates an ordered channel list and rejects empty or duplicate input.
    pub fn new<I>(channels: I) -> Result<Self>
    where
        I: IntoIterator<Item = ChannelName>,
    {
        let channels = channels.into_iter().collect::<Vec<_>>();
        if channels.is_empty() {
            return Err(invalid(
                "create_channel_list",
                "an image channel list must contain at least one channel",
            ));
        }

        let mut unique = BTreeSet::new();
        for channel in &channels {
            if !unique.insert(channel.as_str()) {
                return Err(Error::new(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "image channel names must be unique",
                )
                .with_context(
                    ErrorContext::new("superi-image.channels", "create_channel_list")
                        .with_field("channel", channel.as_str()),
                ));
            }
        }

        let mut layers = vec![LayerName::base()];
        for channel in &channels {
            for layer in channel.layer().prefixes() {
                if !layers.contains(&layer) {
                    layers.push(layer);
                }
            }
        }

        Ok(Self { channels, layers })
    }

    /// Parses qualified names in input order and creates a channel list.
    pub fn from_full_names<I, S>(names: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        names
            .into_iter()
            .map(|name| ChannelName::from_str(name.as_ref()))
            .collect::<Result<Vec<_>>>()
            .and_then(Self::new)
    }

    /// Returns the number of channels.
    #[must_use]
    pub fn len(&self) -> usize {
        self.channels.len()
    }

    /// Returns false because a valid list always contains at least one channel.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.channels.is_empty()
    }

    /// Returns one channel by stable list index.
    #[must_use]
    pub fn get(&self, index: ChannelIndex) -> Option<&ChannelName> {
        self.channels.get(index.get())
    }

    /// Iterates over channels in exact source order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &ChannelName> + DoubleEndedIterator {
        self.channels.iter()
    }

    /// Returns the source index of an exact qualified name.
    #[must_use]
    pub fn index_of(&self, name: &str) -> Option<ChannelIndex> {
        self.channels
            .iter()
            .position(|channel| channel.as_str() == name)
            .map(ChannelIndex::new)
    }

    /// Returns the base and derived nested layers in deterministic order.
    #[must_use]
    pub fn layers(&self) -> &[LayerName] {
        &self.layers
    }

    /// Iterates over channels directly contained in one layer.
    pub fn channels_in_layer<'a>(
        &'a self,
        layer: &'a LayerName,
    ) -> impl DoubleEndedIterator<Item = &'a ChannelName> {
        self.channels
            .iter()
            .filter(move |channel| channel.layer() == layer)
    }

    /// Iterates over channels in one layer and all nested descendants.
    ///
    /// Source channel order is retained. The base layer tree contains every
    /// channel, while [`Self::channels_in_layer`] on the base layer returns only
    /// unqualified channels.
    pub fn channels_in_layer_tree<'a>(
        &'a self,
        layer: &'a LayerName,
    ) -> impl DoubleEndedIterator<Item = &'a ChannelName> {
        self.channels
            .iter()
            .filter(move |channel| channel.layer() == layer || layer.encloses(channel.layer()))
    }

    /// Resolves exact requested names to this list's source indices.
    ///
    /// Request order is preserved, so representation conversions and channel
    /// selections can move data without losing semantic identity.
    pub fn resolve_indices<I, S>(&self, names: I) -> Result<Vec<ChannelIndex>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        names
            .into_iter()
            .map(|name| {
                let name = name.as_ref();
                self.index_of(name).ok_or_else(|| {
                    Error::new(
                        ErrorCategory::NotFound,
                        Recoverability::UserCorrectable,
                        "requested image channel does not exist",
                    )
                    .with_context(
                        ErrorContext::new("superi-image.channels", "resolve_channel_indices")
                            .with_field("channel", name),
                    )
                })
            })
            .collect()
    }
}

impl Index<ChannelIndex> for ChannelList {
    type Output = ChannelName;

    fn index(&self, index: ChannelIndex) -> &Self::Output {
        &self.channels[index.get()]
    }
}

impl<'a> IntoIterator for &'a ChannelList {
    type Item = &'a ChannelName;
    type IntoIter = std::slice::Iter<'a, ChannelName>;

    fn into_iter(self) -> Self::IntoIter {
        self.channels.iter()
    }
}

fn validate_qualified(name: &str, operation: &'static str, kind: &'static str) -> Result<()> {
    if name.is_empty() {
        return Err(invalid(operation, format!("{kind} must not be empty")));
    }
    for component in name.split('.') {
        validate_component(component, operation, kind)?;
    }
    Ok(())
}

fn validate_unqualified(name: &str, operation: &'static str, kind: &'static str) -> Result<()> {
    if name.is_empty() {
        return Err(invalid(operation, format!("{kind} must not be empty")));
    }
    if name.contains('\0') {
        return Err(invalid(
            operation,
            format!("{kind} must not contain a NUL character"),
        ));
    }
    Ok(())
}

fn validate_component(component: &str, operation: &'static str, kind: &'static str) -> Result<()> {
    if component.is_empty() {
        return Err(invalid(
            operation,
            format!("{kind} must not contain empty hierarchy components"),
        ));
    }
    if component.contains('.') {
        return Err(invalid(
            operation,
            format!("{kind} component must not contain the layer delimiter"),
        ));
    }
    if component.contains('\0') {
        return Err(invalid(
            operation,
            format!("{kind} must not contain a NUL character"),
        ));
    }
    Ok(())
}

fn invalid(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-image.channels", operation))
}
