//! Native scanline, tiled, mipmapped, and region-based image access.
//!
//! Access views borrow immutable [`ImageStorage`] bytes without converting,
//! resampling, filling, or renaming them. Scanline images own one complete
//! storage value. Tiled images own independently shareable edge-clipped tiles,
//! including every declared mip level. Region views retain exact half-open
//! coordinates and stable source [`ChannelIndex`] values across both forms.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::PixelBounds;
use superi_core::ids::MediaId;
use superi_core::pixel::AlphaMode;

use crate::channels::{ChannelIndex, ChannelList, ChannelName};
use crate::metadata::{ImageColorTags, ImageMetadata, ImageMetadataValue};
use crate::model::{ChannelStorageLayout, ImageStorage};
use crate::value::ImageSampleType;

const COMPONENT: &str = "superi-image.tiling";

/// Stable logical position of one image inside an identified media sequence.
///
/// File-frame labels and presentation timing remain media I/O concerns. This
/// value keeps the project media identity and zero-based image number attached
/// to every view produced from an access object.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImageSequencePosition {
    media_id: MediaId,
    image_number: u64,
}

impl ImageSequencePosition {
    /// Creates a logical image position within one media source.
    #[must_use]
    pub const fn new(media_id: MediaId, image_number: u64) -> Self {
        Self {
            media_id,
            image_number,
        }
    }

    /// Returns the stable project media identity.
    #[must_use]
    pub const fn media_id(self) -> MediaId {
        self.media_id
    }

    /// Returns the zero-based logical image number.
    #[must_use]
    pub const fn image_number(self) -> u64 {
        self.image_number
    }
}

/// Immutable semantics shared by every storage piece and access view.
///
/// Unlike the dense packed image descriptor, this contract records one sample
/// representation per ordered named channel. It can therefore describe the
/// independently precise planar channels already supported by [`ImageStorage`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageAccessDescriptor {
    data_window: PixelBounds,
    display_window: PixelBounds,
    channels: ChannelList,
    sample_types: Vec<ImageSampleType>,
    color_tags: ImageColorTags,
    alpha_mode: AlphaMode,
    metadata: ImageMetadata,
    sequence_position: Option<ImageSequencePosition>,
}

impl ImageAccessDescriptor {
    /// Creates complete access semantics for one image and all of its levels.
    pub fn new(
        data_window: PixelBounds,
        display_window: PixelBounds,
        channels: ChannelList,
        sample_types: Vec<ImageSampleType>,
        color_space: ColorSpace,
        alpha_mode: AlphaMode,
    ) -> Result<Self> {
        if data_window.is_empty() || display_window.is_empty() {
            return Err(invalid(
                "create_access_descriptor",
                "image data and display windows must be nonempty",
            ));
        }
        if channels.len() != sample_types.len() {
            return Err(invalid(
                "create_access_descriptor",
                "every image channel must have one sample representation",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "compare_descriptor_channels")
                    .with_field("channels", channels.len().to_string())
                    .with_field("sample_types", sample_types.len().to_string()),
            ));
        }
        Ok(Self {
            data_window,
            display_window,
            channels,
            sample_types,
            color_tags: ImageColorTags::new(color_space),
            alpha_mode,
            metadata: ImageMetadata::new(),
            sequence_position: None,
        })
    }

    /// Replaces authoritative color interpretation and retained source color payloads.
    #[must_use]
    pub fn with_color_tags(mut self, color_tags: ImageColorTags) -> Self {
        self.color_tags = color_tags;
        self
    }

    /// Replaces the complete typed and source-specific metadata collection.
    #[must_use]
    pub fn with_image_metadata(mut self, metadata: ImageMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Adds or replaces one losslessly retained image metadata value.
    pub fn with_metadata(
        mut self,
        key: impl Into<String>,
        value: ImageMetadataValue,
    ) -> Result<Self> {
        self.metadata.insert(key, value)?;
        Ok(self)
    }

    /// Attaches a stable logical image-sequence position.
    #[must_use]
    pub fn with_sequence_position(mut self, position: ImageSequencePosition) -> Self {
        self.sequence_position = Some(position);
        self
    }

    /// Returns the signed full-resolution data window.
    #[must_use]
    pub const fn data_window(&self) -> PixelBounds {
        self.data_window
    }

    /// Returns the signed intended display window.
    #[must_use]
    pub const fn display_window(&self) -> PixelBounds {
        self.display_window
    }

    /// Returns channels in stable logical order with exact layer names.
    #[must_use]
    pub const fn channels(&self) -> &ChannelList {
        &self.channels
    }

    /// Returns one channel's native sample representation.
    #[must_use]
    pub fn sample_type(&self, channel: ChannelIndex) -> Option<ImageSampleType> {
        self.sample_types.get(channel.get()).copied()
    }

    /// Returns native channel sample representations in logical order.
    #[must_use]
    pub fn sample_types(&self) -> &[ImageSampleType] {
        &self.sample_types
    }

    /// Returns the unchanged color interpretation.
    #[must_use]
    pub const fn color_space(&self) -> ColorSpace {
        self.color_tags.interpretation()
    }

    /// Returns authoritative color interpretation and preserved source payloads.
    #[must_use]
    pub const fn color_tags(&self) -> &ImageColorTags {
        &self.color_tags
    }

    /// Returns the unchanged alpha interpretation.
    #[must_use]
    pub const fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }

    /// Returns losslessly retained image metadata.
    #[must_use]
    pub const fn metadata(&self) -> &ImageMetadata {
        &self.metadata
    }

    /// Returns logical sequence identity when this image belongs to a sequence.
    #[must_use]
    pub const fn sequence_position(&self) -> Option<ImageSequencePosition> {
        self.sequence_position
    }
}

/// Physical access organization retained by an [`ImageAccess`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ImageOrganization {
    /// One complete, randomly addressable scanline storage value.
    Scanline,
    /// Independently owned rectangular tiles at one or more levels.
    Tiled,
}

/// How non-power-of-two mip level dimensions are rounded.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum LevelRoundingMode {
    /// Round each full-resolution dimension divided by a power of two down.
    Down,
    /// Round each full-resolution dimension divided by a power of two up.
    Up,
}

impl LevelRoundingMode {
    const fn code(self) -> &'static str {
        match self {
            Self::Down => "down",
            Self::Up => "up",
        }
    }
}

/// Whether tiled storage contains only level zero or a complete mip chain.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum MipMode {
    /// The image contains only the full-resolution level.
    SingleLevel,
    /// Each successive level halves both dimensions until reaching one by one.
    Mipmap,
}

impl MipMode {
    const fn code(self) -> &'static str {
        match self {
            Self::SingleLevel => "single_level",
            Self::Mipmap => "mipmap",
        }
    }
}

/// Fixed tile dimensions and mip level geometry for one tiled image.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TileDescription {
    width: u32,
    height: u32,
    mip_mode: MipMode,
    rounding_mode: LevelRoundingMode,
}

impl TileDescription {
    /// Creates a tile description with nonzero fixed dimensions.
    pub fn new(
        width: u32,
        height: u32,
        mip_mode: MipMode,
        rounding_mode: LevelRoundingMode,
    ) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(invalid(
                "create_tile_description",
                "image tile dimensions must be greater than zero",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "tile_dimensions")
                    .with_field("width", width.to_string())
                    .with_field("height", height.to_string()),
            ));
        }
        Ok(Self {
            width,
            height,
            mip_mode,
            rounding_mode,
        })
    }

    /// Returns the fixed nominal tile width in pixels.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    /// Returns the fixed nominal tile height in pixels.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }

    /// Returns whether this image contains a complete mip chain.
    #[must_use]
    pub const fn mip_mode(self) -> MipMode {
        self.mip_mode
    }

    /// Returns the declared odd-dimension rounding mode.
    #[must_use]
    pub const fn rounding_mode(self) -> LevelRoundingMode {
        self.rounding_mode
    }
}

/// Zero-based mip level identity, where level zero is full resolution.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MipLevel(u32);

impl MipLevel {
    /// The full-resolution level.
    pub const BASE: Self = Self(0);

    /// Creates a level from its zero-based number.
    #[must_use]
    pub const fn new(level: u32) -> Self {
        Self(level)
    }

    /// Returns the zero-based level number.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Zero-based tile column and row within one mip level.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TileIndex {
    x: u32,
    y: u32,
}

impl TileIndex {
    /// Creates a tile coordinate from zero-based column and row values.
    #[must_use]
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }

    /// Returns the zero-based tile column.
    #[must_use]
    pub const fn x(self) -> u32 {
        self.x
    }

    /// Returns the zero-based tile row.
    #[must_use]
    pub const fn y(self) -> u32 {
        self.y
    }
}

impl Ord for TileIndex {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.y, self.x).cmp(&(other.y, other.x))
    }
}

impl PartialOrd for TileIndex {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// One independently owned tile at a specific mip level and coordinate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageTile {
    level: MipLevel,
    index: TileIndex,
    storage: ImageStorage,
}

impl ImageTile {
    /// Creates a tile that will be validated when constructing tiled access.
    #[must_use]
    pub const fn new(level: MipLevel, index: TileIndex, storage: ImageStorage) -> Self {
        Self {
            level,
            index,
            storage,
        }
    }

    /// Returns this tile's mip level.
    #[must_use]
    pub const fn level(&self) -> MipLevel {
        self.level
    }

    /// Returns this tile's coordinate within its level.
    #[must_use]
    pub const fn index(&self) -> TileIndex {
        self.index
    }

    /// Returns exact immutable byte storage for this edge-clipped tile.
    #[must_use]
    pub const fn storage(&self) -> &ImageStorage {
        &self.storage
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ImageBacking {
    Scanline(ImageStorage),
    Tiled {
        description: TileDescription,
        level_count: u32,
        tiles: BTreeMap<(MipLevel, TileIndex), ImageTile>,
    },
}

/// Validated immutable access to one scanline or tiled image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageAccess {
    descriptor: ImageAccessDescriptor,
    backing: ImageBacking,
}

impl ImageAccess {
    /// Creates scanline access over one complete full-resolution storage value.
    pub fn from_scanline(descriptor: ImageAccessDescriptor, storage: ImageStorage) -> Result<Self> {
        if storage.bounds() != descriptor.data_window() {
            return Err(invalid(
                "create_scanline_access",
                "scanline storage bounds must equal the image data window",
            )
            .with_context(storage_bounds_context(&storage, descriptor.data_window())));
        }
        validate_storage_semantics(&descriptor, &storage, "create_scanline_access")?;
        Ok(Self {
            descriptor,
            backing: ImageBacking::Scanline(storage),
        })
    }

    /// Creates tiled access and validates a complete, non-overlapping tile set.
    ///
    /// Input order is irrelevant. Successful construction stores tiles in
    /// deterministic level, row, and column order.
    pub fn tiled(
        descriptor: ImageAccessDescriptor,
        tile_description: TileDescription,
        input_tiles: Vec<ImageTile>,
    ) -> Result<Self> {
        let level_count = mip_level_count(descriptor.data_window(), tile_description);
        let expected_tile_count = total_tile_count(
            descriptor.data_window(),
            tile_description,
            level_count,
            "create_tiled_access",
        )?;
        if input_tiles.len() != expected_tile_count {
            return Err(invalid(
                "create_tiled_access",
                "tiled image must contain exactly every declared level tile",
            )
            .with_context(description_context(tile_description))
            .with_context(
                ErrorContext::new(COMPONENT, "compare_tile_count")
                    .with_field("expected", expected_tile_count.to_string())
                    .with_field("actual", input_tiles.len().to_string()),
            ));
        }
        let mut tiles = BTreeMap::new();
        let mut layout_reference = None;

        for tile in input_tiles {
            let expected_bounds = expected_tile_bounds(
                descriptor.data_window(),
                tile_description,
                level_count,
                tile.level,
                tile.index,
                "create_tiled_access",
            )?;
            if tile.storage.bounds() != expected_bounds {
                return Err(invalid(
                    "create_tiled_access",
                    "tile storage bounds do not match its declared level and coordinate",
                )
                .with_context(tile_context(tile.level, tile.index))
                .with_context(storage_bounds_context(&tile.storage, expected_bounds)));
            }
            validate_storage_semantics(&descriptor, &tile.storage, "create_tiled_access")?;
            if let Some(reference) = &layout_reference {
                validate_matching_layout(reference, &tile.storage, tile.level, tile.index)?;
            } else {
                layout_reference = Some(tile.storage.clone());
            }
            let key = (tile.level, tile.index);
            if tiles.insert(key, tile).is_some() {
                return Err(invalid(
                    "create_tiled_access",
                    "tiled image contains a duplicate tile coordinate",
                )
                .with_context(tile_context(key.0, key.1)));
            }
        }

        for level_number in 0..level_count {
            let level = MipLevel::new(level_number);
            let (columns, rows) = tile_count_for_level(
                descriptor.data_window(),
                tile_description,
                level_count,
                level,
                "create_tiled_access",
            )?;
            for y in 0..rows {
                for x in 0..columns {
                    let index = TileIndex::new(x, y);
                    if !tiles.contains_key(&(level, index)) {
                        return Err(invalid(
                            "create_tiled_access",
                            "tiled image is missing required tile storage",
                        )
                        .with_context(tile_context(level, index)));
                    }
                }
            }
        }

        Ok(Self {
            descriptor,
            backing: ImageBacking::Tiled {
                description: tile_description,
                level_count,
                tiles,
            },
        })
    }

    /// Returns immutable image semantics shared by all access views.
    #[must_use]
    pub const fn descriptor(&self) -> &ImageAccessDescriptor {
        &self.descriptor
    }

    /// Returns the retained physical access organization.
    #[must_use]
    pub const fn organization(&self) -> ImageOrganization {
        match self.backing {
            ImageBacking::Scanline(_) => ImageOrganization::Scanline,
            ImageBacking::Tiled { .. } => ImageOrganization::Tiled,
        }
    }

    /// Returns tile and mip geometry for tiled access.
    #[must_use]
    pub const fn tile_description(&self) -> Option<TileDescription> {
        match self.backing {
            ImageBacking::Scanline(_) => None,
            ImageBacking::Tiled { description, .. } => Some(description),
        }
    }

    /// Returns the number of independently addressable levels.
    #[must_use]
    pub fn level_count(&self) -> usize {
        match self.backing {
            ImageBacking::Scanline(_) => 1,
            ImageBacking::Tiled { level_count, .. } => level_count as usize,
        }
    }

    /// Iterates level identities from full to lowest resolution.
    pub fn levels(&self) -> impl ExactSizeIterator<Item = MipLevel> {
        (0..u32::try_from(self.level_count()).expect("image level count fits u32"))
            .map(MipLevel::new)
    }

    /// Returns exact signed bounds for one level.
    pub fn level_bounds(&self, level: MipLevel) -> Result<PixelBounds> {
        let level_count = u32::try_from(self.level_count()).expect("image level count fits u32");
        validate_level(level, level_count, "get_level_bounds")?;
        let description = self.tile_description().unwrap_or(TileDescription {
            width: 1,
            height: 1,
            mip_mode: MipMode::SingleLevel,
            rounding_mode: LevelRoundingMode::Down,
        });
        level_bounds(self.descriptor.data_window(), description, level)
    }

    /// Returns the shared logical channel storage organization.
    #[must_use]
    pub fn storage_layout(&self) -> ChannelStorageLayout {
        match &self.backing {
            ImageBacking::Scanline(storage) => storage.layout(),
            ImageBacking::Tiled { tiles, .. } => tiles
                .first_key_value()
                .expect("validated tiled access contains storage")
                .1
                .storage
                .layout(),
        }
    }

    /// Returns complete native storage for scanline organization.
    ///
    /// Tiled access returns `None` because each tile owns separate storage.
    #[must_use]
    pub const fn scanline_storage(&self) -> Option<&ImageStorage> {
        match &self.backing {
            ImageBacking::Scanline(storage) => Some(storage),
            ImageBacking::Tiled { .. } => None,
        }
    }

    /// Returns tiles for one level in deterministic row-major order.
    pub fn tiles(&self, level: MipLevel) -> Result<Vec<&ImageTile>> {
        let ImageBacking::Tiled {
            level_count, tiles, ..
        } = &self.backing
        else {
            return Err(unsupported(
                "get_level_tiles",
                "scanline image access does not contain tiles",
            ));
        };
        validate_level(level, *level_count, "get_level_tiles")?;
        Ok(tiles
            .iter()
            .filter_map(|((tile_level, _), tile)| (*tile_level == level).then_some(tile))
            .collect())
    }

    /// Returns one exact tile without copying its storage.
    pub fn tile(&self, level: MipLevel, index: TileIndex) -> Result<&ImageTile> {
        let ImageBacking::Tiled {
            level_count, tiles, ..
        } = &self.backing
        else {
            return Err(unsupported(
                "get_tile",
                "scanline image access does not contain tiles",
            ));
        };
        validate_level(level, *level_count, "get_tile")?;
        tiles.get(&(level, index)).ok_or_else(|| {
            not_found("get_tile", "requested image tile does not exist")
                .with_context(tile_context(level, index))
        })
    }

    /// Returns row-major tile coordinates intersecting an exact region.
    pub fn tiles_covering_region(
        &self,
        level: MipLevel,
        bounds: PixelBounds,
    ) -> Result<Vec<TileIndex>> {
        let ImageBacking::Tiled {
            description,
            level_count,
            ..
        } = self.backing
        else {
            return Err(unsupported(
                "tiles_covering_region",
                "scanline image access does not contain tiles",
            ));
        };
        let level_bounds = self.level_bounds(level)?;
        validate_region_bounds(bounds, level_bounds, level, "tiles_covering_region")?;
        validate_level(level, level_count, "tiles_covering_region")?;

        let local_min_x =
            u32::try_from(i64::from(bounds.min_x()) - i64::from(level_bounds.min_x()))
                .expect("validated region has nonnegative local x");
        let local_min_y =
            u32::try_from(i64::from(bounds.min_y()) - i64::from(level_bounds.min_y()))
                .expect("validated region has nonnegative local y");
        let local_max_x =
            u32::try_from(i64::from(bounds.max_x() - 1) - i64::from(level_bounds.min_x()))
                .expect("validated nonempty region has a local maximum x");
        let local_max_y =
            u32::try_from(i64::from(bounds.max_y() - 1) - i64::from(level_bounds.min_y()))
                .expect("validated nonempty region has a local maximum y");
        let first_x = local_min_x / description.width;
        let first_y = local_min_y / description.height;
        let last_x = local_max_x / description.width;
        let last_y = local_max_y / description.height;
        let mut result = Vec::new();
        for y in first_y..=last_y {
            for x in first_x..=last_x {
                result.push(TileIndex::new(x, y));
            }
        }
        Ok(result)
    }

    /// Borrows an exact region and ordered subset of stable source channels.
    pub fn region(
        &self,
        level: MipLevel,
        bounds: PixelBounds,
        channels: &[ChannelIndex],
    ) -> Result<ImageRegion<'_>> {
        let level_bounds = self.level_bounds(level)?;
        validate_region_bounds(bounds, level_bounds, level, "get_image_region")?;
        validate_channel_selection(&self.descriptor, channels, "get_image_region")?;
        Ok(ImageRegion {
            access: self,
            level,
            bounds,
            channels: channels.to_vec(),
        })
    }

    /// Borrows an exact region with every channel in source order.
    pub fn region_all(&self, level: MipLevel, bounds: PixelBounds) -> Result<ImageRegion<'_>> {
        let channels = (0..self.descriptor.channels().len())
            .map(ChannelIndex::new)
            .collect::<Vec<_>>();
        self.region(level, bounds, &channels)
    }

    /// Borrows an exact region with exact named channels in request order.
    pub fn region_by_names<I, S>(
        &self,
        level: MipLevel,
        bounds: PixelBounds,
        names: I,
    ) -> Result<ImageRegion<'_>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let channels = self.descriptor.channels().resolve_indices(names)?;
        self.region(level, bounds, &channels)
    }

    /// Borrows one complete scanline with an ordered channel subset.
    pub fn scanline(&self, y: i32, channels: &[ChannelIndex]) -> Result<ImageRegion<'_>> {
        if !matches!(self.backing, ImageBacking::Scanline(_)) {
            return Err(unsupported(
                "get_scanline",
                "tiled image access requires tile or region access",
            ));
        }
        let bounds = self.descriptor.data_window();
        if y < bounds.min_y() || y >= bounds.max_y() {
            return Err(invalid(
                "get_scanline",
                "requested scanline lies outside the image data window",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "scanline_coordinate").with_field("y", y.to_string()),
            ));
        }
        let scanline = PixelBounds::new(bounds.min_x(), y, bounds.max_x(), y + 1)
            .expect("validated scanline edges remain ordered");
        self.region(MipLevel::BASE, scanline, channels)
    }

    fn sample_bytes(
        &self,
        level: MipLevel,
        channel: ChannelIndex,
        x: i32,
        y: i32,
    ) -> Option<&[u8]> {
        match &self.backing {
            ImageBacking::Scanline(storage) => storage.sample_bytes(channel.get(), x, y),
            ImageBacking::Tiled {
                description, tiles, ..
            } => {
                let level_bounds = self.level_bounds(level).ok()?;
                if !level_bounds.contains(x, y) {
                    return None;
                }
                let local_x = u32::try_from(i64::from(x) - i64::from(level_bounds.min_x())).ok()?;
                let local_y = u32::try_from(i64::from(y) - i64::from(level_bounds.min_y())).ok()?;
                let index =
                    TileIndex::new(local_x / description.width, local_y / description.height);
                tiles
                    .get(&(level, index))?
                    .storage
                    .sample_bytes(channel.get(), x, y)
            }
        }
    }
}

/// Borrowed exact region with stable source-channel selection.
///
/// The view never owns or transforms sample bytes. A tiled region may cross
/// multiple independently allocated tiles while presenting one coordinate and
/// channel access contract.
#[derive(Debug)]
pub struct ImageRegion<'a> {
    access: &'a ImageAccess,
    level: MipLevel,
    bounds: PixelBounds,
    channels: Vec<ChannelIndex>,
}

impl ImageRegion<'_> {
    /// Returns the source access value that owns all borrowed bytes.
    #[must_use]
    pub const fn access(&self) -> &ImageAccess {
        self.access
    }

    /// Returns the unchanged mip level identity.
    #[must_use]
    pub const fn level(&self) -> MipLevel {
        self.level
    }

    /// Returns the exact requested half-open region.
    #[must_use]
    pub const fn bounds(&self) -> PixelBounds {
        self.bounds
    }

    /// Returns stable source channel indices in exact request order.
    #[must_use]
    pub fn selected_channels(&self) -> &[ChannelIndex] {
        &self.channels
    }

    /// Iterates exact channel and nested-layer names in request order.
    pub fn channel_names(&self) -> impl ExactSizeIterator<Item = &ChannelName> {
        self.channels.iter().map(|&channel| {
            self.access
                .descriptor
                .channels()
                .get(channel)
                .expect("validated image region channel exists")
        })
    }

    /// Returns original native bytes for one selected channel sample.
    ///
    /// `None` means the channel was not selected or the coordinate lies outside
    /// this exact region. No fill value or implicit storage conversion occurs.
    #[must_use]
    pub fn sample_bytes(&self, channel: ChannelIndex, x: i32, y: i32) -> Option<&[u8]> {
        if !self.bounds.contains(x, y) || !self.channels.contains(&channel) {
            return None;
        }
        self.access.sample_bytes(self.level, channel, x, y)
    }
}

fn validate_storage_semantics(
    descriptor: &ImageAccessDescriptor,
    storage: &ImageStorage,
    operation: &'static str,
) -> Result<()> {
    if storage.channel_count() != descriptor.channels().len() {
        return Err(invalid(
            operation,
            "storage channel count does not match image access semantics",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "compare_storage_channels")
                .with_field("storage", storage.channel_count().to_string())
                .with_field("descriptor", descriptor.channels().len().to_string()),
        ));
    }
    for (index, channel) in storage.channels().iter().copied().enumerate() {
        let channel_index = ChannelIndex::new(index);
        let sample_type = descriptor
            .sample_type(channel_index)
            .expect("validated descriptor channel count matches storage");
        let expected_bytes = usize::from(sample_type.bits()) / 8;
        if channel.sample_bytes() != expected_bytes {
            return Err(invalid(
                operation,
                "storage channel precision does not match image access semantics",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "compare_channel_precision")
                    .with_field("channel_index", index.to_string())
                    .with_field("sample_type", sample_type.code())
                    .with_field("expected_bytes", expected_bytes.to_string())
                    .with_field("actual_bytes", channel.sample_bytes().to_string()),
            ));
        }
    }
    Ok(())
}

fn validate_matching_layout(
    reference: &ImageStorage,
    storage: &ImageStorage,
    level: MipLevel,
    index: TileIndex,
) -> Result<()> {
    let same_channels = reference
        .channels()
        .iter()
        .zip(storage.channels())
        .all(|(left, right)| left == right);
    if reference.layout() != storage.layout()
        || reference.plane_count() != storage.plane_count()
        || reference.channel_count() != storage.channel_count()
        || !same_channels
    {
        return Err(invalid(
            "create_tiled_access",
            "every image tile must retain the same channel storage layout",
        )
        .with_context(tile_context(level, index)));
    }
    Ok(())
}

fn validate_channel_selection(
    descriptor: &ImageAccessDescriptor,
    channels: &[ChannelIndex],
    operation: &'static str,
) -> Result<()> {
    if channels.is_empty() {
        return Err(invalid(
            operation,
            "image region access requires at least one selected channel",
        ));
    }
    let mut unique = BTreeSet::new();
    for &channel in channels {
        if descriptor.channels().get(channel).is_none() {
            return Err(
                not_found(operation, "selected image channel does not exist").with_context(
                    ErrorContext::new(COMPONENT, "selected_channel")
                        .with_field("channel_index", channel.get().to_string()),
                ),
            );
        }
        if !unique.insert(channel) {
            return Err(invalid(
                operation,
                "image region channel selection must not contain duplicates",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "selected_channel")
                    .with_field("channel_index", channel.get().to_string()),
            ));
        }
    }
    Ok(())
}

fn validate_region_bounds(
    requested: PixelBounds,
    available: PixelBounds,
    level: MipLevel,
    operation: &'static str,
) -> Result<()> {
    if requested.is_empty() || requested.intersection(available) != Some(requested) {
        return Err(invalid(
            operation,
            "requested image region must be nonempty and inside its level",
        )
        .with_context(level_context(level, available))
        .with_context(requested_bounds_context(requested)));
    }
    Ok(())
}

fn mip_level_count(data_window: PixelBounds, description: TileDescription) -> u32 {
    if description.mip_mode == MipMode::SingleLevel {
        return 1;
    }
    let mut size = data_window.width().max(data_window.height());
    let mut count = 1;
    while size > 1 {
        size = halve(size, description.rounding_mode);
        count += 1;
    }
    count
}

fn halve(value: u32, rounding: LevelRoundingMode) -> u32 {
    match rounding {
        LevelRoundingMode::Down => (value / 2).max(1),
        LevelRoundingMode::Up => value.div_ceil(2),
    }
}

fn level_size(mut base: u32, level: MipLevel, rounding: LevelRoundingMode) -> u32 {
    for _ in 0..level.get() {
        base = halve(base, rounding);
    }
    base
}

fn level_bounds(
    data_window: PixelBounds,
    description: TileDescription,
    level: MipLevel,
) -> Result<PixelBounds> {
    let width = level_size(data_window.width(), level, description.rounding_mode);
    let height = level_size(data_window.height(), level, description.rounding_mode);
    PixelBounds::from_origin_size(data_window.min_x(), data_window.min_y(), width, height).map_err(
        |_| {
            exhausted(
                "calculate_mip_bounds",
                "mip level bounds exceed the supported coordinate range",
            )
            .with_context(level_context(level, data_window))
        },
    )
}

fn tile_count_for_level(
    data_window: PixelBounds,
    description: TileDescription,
    level_count: u32,
    level: MipLevel,
    operation: &'static str,
) -> Result<(u32, u32)> {
    validate_level(level, level_count, operation)?;
    let bounds = level_bounds(data_window, description, level)?;
    Ok((
        bounds.width().div_ceil(description.width),
        bounds.height().div_ceil(description.height),
    ))
}

fn total_tile_count(
    data_window: PixelBounds,
    description: TileDescription,
    level_count: u32,
    operation: &'static str,
) -> Result<usize> {
    let mut total = 0_u128;
    for level in 0..level_count {
        let (columns, rows) = tile_count_for_level(
            data_window,
            description,
            level_count,
            MipLevel::new(level),
            operation,
        )?;
        total = total
            .checked_add(u128::from(columns) * u128::from(rows))
            .ok_or_else(|| {
                exhausted(
                    operation,
                    "image tile count exceeds the supported address space",
                )
                .with_context(description_context(description))
            })?;
    }
    usize::try_from(total).map_err(|_| {
        exhausted(
            operation,
            "image tile count cannot be represented on this platform",
        )
        .with_context(description_context(description))
    })
}

fn expected_tile_bounds(
    data_window: PixelBounds,
    description: TileDescription,
    level_count: u32,
    level: MipLevel,
    index: TileIndex,
    operation: &'static str,
) -> Result<PixelBounds> {
    validate_level(level, level_count, operation)?;
    let bounds = level_bounds(data_window, description, level)?;
    let (columns, rows) =
        tile_count_for_level(data_window, description, level_count, level, operation)?;
    if index.x >= columns || index.y >= rows {
        return Err(
            invalid(operation, "tile coordinate lies outside its mip level")
                .with_context(tile_context(level, index))
                .with_context(
                    ErrorContext::new(COMPONENT, "level_tile_count")
                        .with_field("columns", columns.to_string())
                        .with_field("rows", rows.to_string()),
                ),
        );
    }
    let local_x = index.x.checked_mul(description.width).ok_or_else(|| {
        exhausted(operation, "tile horizontal coordinate overflowed")
            .with_context(tile_context(level, index))
    })?;
    let local_y = index.y.checked_mul(description.height).ok_or_else(|| {
        exhausted(operation, "tile vertical coordinate overflowed")
            .with_context(tile_context(level, index))
    })?;
    let min_x = i32::try_from(i64::from(bounds.min_x()) + i64::from(local_x)).map_err(|_| {
        exhausted(operation, "tile horizontal bounds overflowed")
            .with_context(tile_context(level, index))
    })?;
    let min_y = i32::try_from(i64::from(bounds.min_y()) + i64::from(local_y)).map_err(|_| {
        exhausted(operation, "tile vertical bounds overflowed")
            .with_context(tile_context(level, index))
    })?;
    let width = description.width.min(bounds.width() - local_x);
    let height = description.height.min(bounds.height() - local_y);
    PixelBounds::from_origin_size(min_x, min_y, width, height).map_err(|_| {
        exhausted(
            operation,
            "tile bounds exceed the supported coordinate range",
        )
        .with_context(tile_context(level, index))
    })
}

fn validate_level(level: MipLevel, level_count: u32, operation: &'static str) -> Result<()> {
    if level.get() >= level_count {
        return Err(
            not_found(operation, "requested image mip level does not exist").with_context(
                ErrorContext::new(COMPONENT, "mip_level")
                    .with_field("level", level.get().to_string())
                    .with_field("level_count", level_count.to_string()),
            ),
        );
    }
    Ok(())
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
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

fn not_found(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::NotFound,
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

fn tile_context(level: MipLevel, index: TileIndex) -> ErrorContext {
    ErrorContext::new(COMPONENT, "tile_address")
        .with_field("level", level.get().to_string())
        .with_field("tile_x", index.x().to_string())
        .with_field("tile_y", index.y().to_string())
}

fn level_context(level: MipLevel, bounds: PixelBounds) -> ErrorContext {
    ErrorContext::new(COMPONENT, "level_bounds")
        .with_field("level", level.get().to_string())
        .with_field("min_x", bounds.min_x().to_string())
        .with_field("min_y", bounds.min_y().to_string())
        .with_field("max_x", bounds.max_x().to_string())
        .with_field("max_y", bounds.max_y().to_string())
}

fn storage_bounds_context(storage: &ImageStorage, expected: PixelBounds) -> ErrorContext {
    let actual = storage.bounds();
    ErrorContext::new(COMPONENT, "storage_bounds")
        .with_field("actual_min_x", actual.min_x().to_string())
        .with_field("actual_min_y", actual.min_y().to_string())
        .with_field("actual_max_x", actual.max_x().to_string())
        .with_field("actual_max_y", actual.max_y().to_string())
        .with_field("expected_min_x", expected.min_x().to_string())
        .with_field("expected_min_y", expected.min_y().to_string())
        .with_field("expected_max_x", expected.max_x().to_string())
        .with_field("expected_max_y", expected.max_y().to_string())
}

fn requested_bounds_context(bounds: PixelBounds) -> ErrorContext {
    ErrorContext::new(COMPONENT, "requested_region")
        .with_field("min_x", bounds.min_x().to_string())
        .with_field("min_y", bounds.min_y().to_string())
        .with_field("max_x", bounds.max_x().to_string())
        .with_field("max_y", bounds.max_y().to_string())
}

fn description_context(description: TileDescription) -> ErrorContext {
    ErrorContext::new(COMPONENT, "tile_description")
        .with_field("tile_width", description.width().to_string())
        .with_field("tile_height", description.height().to_string())
        .with_field("mip_mode", description.mip_mode().code())
        .with_field("rounding_mode", description.rounding_mode().code())
}
