//! Bounded still-image decoding and representation-aware encoding.
//!
//! The public boundary is [`ImageAccess`], so file I/O retains exact channel
//! names, native sample representations, alpha and color interpretation,
//! metadata, signed extents, storage organization, and sequence identity. A
//! writer rejects values its target format cannot represent instead of
//! silently converting them.

use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;

use image::codecs::bmp::BmpEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::tga::TgaEncoder;
use image::codecs::tiff::TiffEncoder;
use image::codecs::webp::WebPEncoder;
use image::{ColorType, ExtendedColorType, ImageDecoder, ImageEncoder, ImageFormat, ImageReader};
use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::PixelBounds;
use superi_core::pixel::AlphaMode;

use crate::channels::{ChannelIndex, ChannelList};
use crate::metadata::{ImageColorTags, ImageMetadata, ImageMetadataValue, ImageOrientation};
use crate::model::{ByteAlignment, ChannelSlice, ChannelStorageLayout, ImageStorage, StoragePlane};
use crate::tiling::{
    ImageAccess, ImageAccessDescriptor, ImageOrganization, ImageSequencePosition, ImageTile,
    LevelRoundingMode, MipLevel, MipMode, TileDescription, TileIndex,
};
use crate::value::ImageSampleType;

const COMPONENT: &str = "superi-image.io";
const EXIF_KEY: &str = "io.exif";
const ORIGINAL_COLOR_KEY: &str = "io.original_color_type";

/// A still-image representation supported by this module.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum StillImageFormat {
    /// OpenEXR multipart high-dynamic-range image data.
    Exr,
    /// SMPTE ST 268 Digital Picture Exchange data.
    Dpx,
    /// Portable Network Graphics data.
    Png,
    /// JPEG image data.
    Jpeg,
    /// Tagged Image File Format data.
    Tiff,
    /// WebP image data.
    WebP,
    /// Truevision TGA image data.
    Tga,
    /// Windows bitmap image data.
    Bmp,
}

impl StillImageFormat {
    /// Every still-image representation supported by this version.
    pub const ALL: &'static [Self] = &[
        Self::Exr,
        Self::Dpx,
        Self::Png,
        Self::Jpeg,
        Self::Tiff,
        Self::WebP,
        Self::Tga,
        Self::Bmp,
    ];

    /// Returns the permanent lowercase format code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Exr => "exr",
            Self::Dpx => "dpx",
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Tiff => "tiff",
            Self::WebP => "webp",
            Self::Tga => "tga",
            Self::Bmp => "bmp",
        }
    }

    /// Recognizes a case-insensitive file extension, with or without a period.
    #[must_use]
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str()
        {
            "exr" => Some(Self::Exr),
            "dpx" => Some(Self::Dpx),
            "png" => Some(Self::Png),
            "jpg" | "jpeg" | "jpe" => Some(Self::Jpeg),
            "tif" | "tiff" => Some(Self::Tiff),
            "webp" => Some(Self::WebP),
            "tga" | "vda" | "icb" | "vst" => Some(Self::Tga),
            "bmp" | "dib" => Some(Self::Bmp),
            _ => None,
        }
    }

    fn image_format(self) -> Option<ImageFormat> {
        match self {
            Self::Png => Some(ImageFormat::Png),
            Self::Jpeg => Some(ImageFormat::Jpeg),
            Self::Tiff => Some(ImageFormat::Tiff),
            Self::WebP => Some(ImageFormat::WebP),
            Self::Tga => Some(ImageFormat::Tga),
            Self::Bmp => Some(ImageFormat::Bmp),
            Self::Exr | Self::Dpx => None,
        }
    }
}

/// Limits and external identity applied while decoding one still image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadOptions {
    max_width: u32,
    max_height: u32,
    max_decoded_bytes: u64,
    sequence_position: Option<ImageSequencePosition>,
}

impl ReadOptions {
    /// Creates decode options with explicit resource limits.
    pub fn new(max_width: u32, max_height: u32, max_decoded_bytes: u64) -> Result<Self> {
        if max_width == 0 || max_height == 0 || max_decoded_bytes == 0 {
            return Err(invalid(
                "create_read_options",
                "still-image decode limits must be greater than zero",
            ));
        }
        Ok(Self {
            max_width,
            max_height,
            max_decoded_bytes,
            sequence_position: None,
        })
    }

    /// Attaches caller-owned media sequence identity to the decoded image.
    #[must_use]
    pub const fn with_sequence_position(mut self, position: ImageSequencePosition) -> Self {
        self.sequence_position = Some(position);
        self
    }

    /// Returns the maximum accepted image width.
    #[must_use]
    pub const fn max_width(self) -> u32 {
        self.max_width
    }

    /// Returns the maximum accepted image height.
    #[must_use]
    pub const fn max_height(self) -> u32 {
        self.max_height
    }

    /// Returns the maximum decoded byte allocation.
    #[must_use]
    pub const fn max_decoded_bytes(self) -> u64 {
        self.max_decoded_bytes
    }

    /// Returns the external sequence identity applied to each decoded layer.
    #[must_use]
    pub const fn sequence_position(self) -> Option<ImageSequencePosition> {
        self.sequence_position
    }
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            max_width: 65_536,
            max_height: 65_536,
            max_decoded_bytes: 4 * 1024 * 1024 * 1024,
            sequence_position: None,
        }
    }
}

/// Endianness used for DPX headers and packed image words.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum DpxEndianness {
    /// Big-endian `SDPX` data.
    #[default]
    Big,
    /// Little-endian `XPDS` data.
    Little,
}

/// DPX integer sample packing within 32-bit words.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum DpxPacking {
    /// Filled method A, with component bits at the most significant end.
    #[default]
    FilledMethodA,
    /// Filled method B, with component bits after leading pad bits.
    FilledMethodB,
}

/// Encoding policy for one still-image write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WriteOptions {
    jpeg_quality: u8,
    dpx_endianness: DpxEndianness,
    dpx_packing: DpxPacking,
    dpx_bit_depth: u8,
}

impl WriteOptions {
    /// Replaces JPEG quality in the inclusive range 1 through 100.
    pub fn with_jpeg_quality(mut self, quality: u8) -> Result<Self> {
        if !(1..=100).contains(&quality) {
            return Err(invalid(
                "set_jpeg_quality",
                "JPEG quality must be in the inclusive range 1 through 100",
            ));
        }
        self.jpeg_quality = quality;
        Ok(self)
    }

    /// Selects DPX byte order, packing, and integer precision.
    pub fn with_dpx(
        mut self,
        endianness: DpxEndianness,
        packing: DpxPacking,
        bit_depth: u8,
    ) -> Result<Self> {
        if !matches!(bit_depth, 8 | 10 | 12 | 16) {
            return Err(invalid(
                "set_dpx_options",
                "DPX output bit depth must be 8, 10, 12, or 16",
            ));
        }
        self.dpx_endianness = endianness;
        self.dpx_packing = packing;
        self.dpx_bit_depth = bit_depth;
        Ok(self)
    }
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            jpeg_quality: 90,
            dpx_endianness: DpxEndianness::Big,
            dpx_packing: DpxPacking::FilledMethodA,
            dpx_bit_depth: 10,
        }
    }
}

/// One named file part and its complete image access contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StillImageLayer {
    name: Option<String>,
    access: ImageAccess,
}

impl StillImageLayer {
    /// Creates a named or unnamed still-image layer.
    pub fn new(name: Option<String>, access: ImageAccess) -> Result<Self> {
        if name
            .as_ref()
            .is_some_and(|name| name.is_empty() || name.contains('\0'))
        {
            return Err(invalid(
                "create_still_image_layer",
                "still-image layer names must be nonempty and contain no NUL bytes",
            ));
        }
        Ok(Self { name, access })
    }

    /// Returns the exact file part name, when present.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the complete immutable image access value.
    #[must_use]
    pub const fn access(&self) -> &ImageAccess {
        &self.access
    }
}

/// One still image containing one or more independently named file parts.
#[derive(Clone, Debug, PartialEq)]
pub struct StillImage {
    layers: Vec<StillImageLayer>,
    original: Option<OriginalRepresentation>,
}

#[derive(Clone, Debug, PartialEq)]
enum OriginalRepresentation {
    Exr(Box<exr::image::AnyImage>),
    Dpx {
        bytes: Arc<[u8]>,
        options: WriteOptions,
    },
    Raster {
        format: StillImageFormat,
        bytes: Arc<[u8]>,
    },
}

impl StillImage {
    /// Creates a still image from one or more validated layers.
    pub fn new(layers: Vec<StillImageLayer>) -> Result<Self> {
        if layers.is_empty() {
            return Err(invalid(
                "create_still_image",
                "a still image requires at least one layer",
            ));
        }
        let mut names = std::collections::BTreeSet::new();
        for layer in &layers {
            if !names.insert(layer.name()) {
                return Err(invalid(
                    "create_still_image",
                    "still-image layer names must be unique",
                ));
            }
        }
        Ok(Self {
            layers,
            original: None,
        })
    }

    /// Wraps one unnamed image access value.
    #[must_use]
    pub fn from_access(access: ImageAccess) -> Self {
        Self {
            layers: vec![StillImageLayer { name: None, access }],
            original: None,
        }
    }

    /// Returns file parts in stable source order.
    #[must_use]
    pub fn layers(&self) -> &[StillImageLayer] {
        &self.layers
    }

    /// Returns the sole image access value or an Unsupported error for multipart data.
    pub fn single_access(&self) -> Result<&ImageAccess> {
        if self.layers.len() != 1 {
            return Err(unsupported(
                "get_single_access",
                "multipart still image does not have one unambiguous access value",
            ));
        }
        Ok(self.layers[0].access())
    }
}

/// Decodes one still image from a buffered, seekable stream.
pub fn read<R: BufRead + Seek>(
    reader: &mut R,
    format: StillImageFormat,
    options: &ReadOptions,
) -> Result<StillImage> {
    match format {
        StillImageFormat::Exr => read_exr(reader, options),
        StillImageFormat::Dpx => read_dpx(reader, options),
        _ => read_raster(reader, format, options),
    }
}

/// Encodes one still image to a seekable stream.
pub fn write<W: Write + Seek>(
    writer: &mut W,
    format: StillImageFormat,
    image: &StillImage,
    options: &WriteOptions,
) -> Result<()> {
    match format {
        StillImageFormat::Exr => write_exr(writer, image),
        StillImageFormat::Dpx => write_dpx(writer, image, options),
        _ => write_raster(writer, format, image, options),
    }
}

/// Decodes a still image after inferring its format from the path extension.
pub fn read_path(path: impl AsRef<Path>, options: &ReadOptions) -> Result<StillImage> {
    let path = path.as_ref();
    let format = path_format(path)?;
    let file = File::open(path).map_err(|source| file_error("open_image", source))?;
    read(&mut BufReader::new(file), format, options)
}

/// Encodes a still image after inferring its format from the path extension.
pub fn write_path(
    path: impl AsRef<Path>,
    image: &StillImage,
    options: &WriteOptions,
) -> Result<()> {
    let path = path.as_ref();
    let format = path_format(path)?;
    let file = File::create(path).map_err(|source| file_error("create_image", source))?;
    write(&mut BufWriter::new(file), format, image, options)
}

fn path_format(path: &Path) -> Result<StillImageFormat> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .and_then(StillImageFormat::from_extension)
        .ok_or_else(|| {
            unsupported(
                "infer_format",
                "unsupported or missing still-image extension",
            )
        })
}

fn read_raster<R: BufRead + Seek>(
    reader: &mut R,
    format: StillImageFormat,
    options: &ReadOptions,
) -> Result<StillImage> {
    const MAX_HEADER_OVERHEAD: u64 = 16 * 1024 * 1024;

    let start = reader
        .stream_position()
        .map_err(|source| file_error("inspect_raster", source))?;
    let image_format = format
        .image_format()
        .expect("raster dispatch has an image crate format");
    let mut decoder = ImageReader::with_format(&mut *reader, image_format)
        .into_decoder()
        .map_err(|source| image_read_error(format, source))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(options.max_width);
    limits.max_image_height = Some(options.max_height);
    limits.max_alloc = Some(options.max_decoded_bytes);
    decoder
        .set_limits(limits)
        .map_err(|source| image_read_error(format, source))?;

    let (width, height) = decoder.dimensions();
    let color_type = decoder.color_type();
    let original_color_type = decoder.original_color_type();
    let total_bytes = decoder.total_bytes();
    if total_bytes > options.max_decoded_bytes {
        return Err(exhausted(
            "decode_raster",
            "decoded image exceeds the configured byte limit",
        ));
    }
    let total_bytes = usize::try_from(total_bytes).map_err(|_| {
        exhausted(
            "decode_raster",
            "decoded image does not fit the host address space",
        )
    })?;
    let icc = decoder
        .icc_profile()
        .map_err(|source| image_read_error(format, source))?;
    let exif = decoder
        .exif_metadata()
        .map_err(|source| image_read_error(format, source))?;
    let orientation = decoder
        .orientation()
        .map_err(|source| image_read_error(format, source))?;
    let mut bytes = vec![0_u8; total_bytes];
    decoder
        .read_image(&mut bytes)
        .map_err(|source| image_read_error(format, source))?;

    let raster = RasterDescription::from_color_type(color_type)?;
    let bounds = PixelBounds::from_origin_size(0, 0, width, height)?;
    let row_stride = checked_product(
        &[width as u64, u64::from(color_type.bytes_per_pixel())],
        "decode_raster",
    )?;
    let alignment =
        ByteAlignment::new(raster.sample_bytes).expect("sample bytes are powers of two");
    let storage = ImageStorage::new(
        bounds,
        ChannelStorageLayout::Interleaved,
        vec![StoragePlane::new(
            Arc::from(bytes),
            0,
            row_stride,
            alignment,
        )?],
        (0..raster.channels.len())
            .map(|channel| {
                ChannelSlice::new(
                    0,
                    channel * raster.sample_bytes,
                    raster.sample_bytes,
                    raster.channels.len() * raster.sample_bytes,
                )
            })
            .collect::<Result<Vec<_>>>()?,
    )?;

    let mut metadata = ImageMetadata::new();
    if let Some(orientation) = map_orientation(orientation) {
        metadata.set_orientation(orientation);
    }
    if let Some(exif) = exif.filter(|bytes| !bytes.is_empty()) {
        metadata.insert(EXIF_KEY, ImageMetadataValue::Bytes(Arc::from(exif)))?;
    }
    metadata.insert(
        ORIGINAL_COLOR_KEY,
        ImageMetadataValue::Text(format!("{original_color_type:?}")),
    )?;
    let mut color_tags = ImageColorTags::new(ColorSpace::SRGB);
    if let Some(icc) = icc.filter(|bytes| !bytes.is_empty()) {
        color_tags = color_tags.with_icc_profile(Arc::from(icc))?;
    }
    let mut descriptor = ImageAccessDescriptor::new(
        bounds,
        bounds,
        ChannelList::from_full_names(raster.channels.iter().copied())?,
        vec![raster.sample_type; raster.channels.len()],
        ColorSpace::SRGB,
        if raster.has_alpha {
            AlphaMode::Straight
        } else {
            AlphaMode::Opaque
        },
    )?
    .with_color_tags(color_tags)
    .with_image_metadata(metadata);
    if let Some(position) = options.sequence_position {
        descriptor = descriptor.with_sequence_position(position);
    }
    let access = ImageAccess::from_scanline(descriptor, storage)?;
    reader
        .seek(SeekFrom::Start(start))
        .map_err(|source| file_error("rewind_raster", source))?;
    let max_file_bytes = options
        .max_decoded_bytes
        .checked_add(MAX_HEADER_OVERHEAD)
        .ok_or_else(|| exhausted("decode_raster", "raster file limit overflows"))?;
    let mut source_bytes = Vec::new();
    (&mut *reader)
        .take(max_file_bytes + 1)
        .read_to_end(&mut source_bytes)
        .map_err(|source| file_error("retain_raster", source))?;
    if source_bytes.len() as u64 > max_file_bytes {
        return Err(exhausted(
            "decode_raster",
            "raster file exceeds the configured file limit",
        ));
    }
    Ok(StillImage {
        layers: vec![StillImageLayer { name: None, access }],
        original: Some(OriginalRepresentation::Raster {
            format,
            bytes: Arc::from(source_bytes),
        }),
    })
}

fn write_raster<W: Write + Seek>(
    writer: &mut W,
    format: StillImageFormat,
    image: &StillImage,
    options: &WriteOptions,
) -> Result<()> {
    if let Some(OriginalRepresentation::Raster {
        format: source_format,
        bytes,
    }) = &image.original
    {
        let options_preserve_source = format != StillImageFormat::Jpeg
            || options.jpeg_quality == WriteOptions::default().jpeg_quality;
        if *source_format == format && options_preserve_source {
            writer
                .write_all(bytes)
                .map_err(|source| file_error("write_raster", source))?;
            return Ok(());
        }
    }
    let access = image.single_access()?;
    let (bytes, color_type) = raster_bytes(access, format)?;
    let bounds = access.descriptor().data_window();
    let icc = access.descriptor().color_tags().icc_profile();
    let result = match format {
        StillImageFormat::Png => {
            let mut encoder = PngEncoder::new(writer);
            set_icc(&mut encoder, icc, format)?;
            encoder.write_image(&bytes, bounds.width(), bounds.height(), color_type)
        }
        StillImageFormat::Jpeg => {
            let mut encoder = JpegEncoder::new_with_quality(writer, options.jpeg_quality);
            set_icc(&mut encoder, icc, format)?;
            encoder.write_image(&bytes, bounds.width(), bounds.height(), color_type)
        }
        StillImageFormat::Tiff => TiffEncoder::new(writer).write_image(
            &bytes,
            bounds.width(),
            bounds.height(),
            color_type,
        ),
        StillImageFormat::WebP => {
            let mut encoder = WebPEncoder::new_lossless(writer);
            set_icc(&mut encoder, icc, format)?;
            encoder.write_image(&bytes, bounds.width(), bounds.height(), color_type)
        }
        StillImageFormat::Tga => {
            TgaEncoder::new(writer).write_image(&bytes, bounds.width(), bounds.height(), color_type)
        }
        StillImageFormat::Bmp => {
            BmpEncoder::new(writer).write_image(&bytes, bounds.width(), bounds.height(), color_type)
        }
        StillImageFormat::Exr | StillImageFormat::Dpx => unreachable!("raster dispatch only"),
    };
    result.map_err(|source| image_write_error(format, source))
}

fn set_icc(
    encoder: &mut impl ImageEncoder,
    icc: Option<&[u8]>,
    format: StillImageFormat,
) -> Result<()> {
    if let Some(icc) = icc {
        encoder.set_icc_profile(icc.to_vec()).map_err(|source| {
            Error::with_source(
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                "target image representation cannot preserve the ICC profile",
                source,
            )
            .with_context(format_context("encode_raster", format))
        })?;
    }
    Ok(())
}

fn raster_bytes(
    access: &ImageAccess,
    format: StillImageFormat,
) -> Result<(Vec<u8>, ExtendedColorType)> {
    let descriptor = access.descriptor();
    let bounds = descriptor.data_window();
    if bounds.min_x() != 0
        || bounds.min_y() != 0
        || descriptor.display_window() != bounds
        || access.level_count() != 1
    {
        return Err(unsupported(
            "encode_raster",
            "target raster representation cannot preserve signed windows, display crop, or mip levels",
        )
        .with_context(format_context("representation", format)));
    }
    let storage = access.scanline_storage().ok_or_else(|| {
        unsupported(
            "encode_raster",
            "target raster representation cannot preserve tiled storage",
        )
    })?;
    let names = descriptor
        .channels()
        .iter()
        .map(|name| name.as_str())
        .collect::<Vec<_>>();
    let has_alpha = matches!(names.as_slice(), ["Y", "A"] | ["R", "G", "B", "A"]);
    let valid_names = matches!(
        names.as_slice(),
        ["Y"] | ["Y", "A"] | ["R", "G", "B"] | ["R", "G", "B", "A"]
    );
    if !valid_names {
        return Err(unsupported(
            "encode_raster",
            "target raster representation requires Y, YA, RGB, or RGBA channel meaning",
        ));
    }
    if has_alpha && descriptor.alpha_mode() != AlphaMode::Straight {
        return Err(unsupported(
            "encode_raster",
            "target raster representation requires straight alpha",
        ));
    }
    if !has_alpha && descriptor.alpha_mode() != AlphaMode::Opaque {
        return Err(unsupported(
            "encode_raster",
            "alpha interpretation requires an alpha channel",
        ));
    }
    if descriptor.color_space() != ColorSpace::SRGB
        && descriptor.color_space() != ColorSpace::UNSPECIFIED
        && descriptor.color_tags().icc_profile().is_none()
    {
        return Err(unsupported(
            "encode_raster",
            "target raster representation cannot preserve the declared color interpretation without an ICC profile",
        ));
    }
    if descriptor.metadata().orientation().is_some()
        || descriptor.metadata().pixel_aspect_ratio().is_some()
        || descriptor.metadata().timecode().is_some()
        || descriptor
            .metadata()
            .iter()
            .any(|(key, _)| key != ORIGINAL_COLOR_KEY)
    {
        return Err(unsupported(
            "encode_raster",
            "target raster encoder cannot preserve all declared image metadata",
        ));
    }
    if matches!(
        format,
        StillImageFormat::Tiff | StillImageFormat::Tga | StillImageFormat::Bmp
    ) && descriptor.color_tags().icc_profile().is_some()
    {
        return Err(unsupported(
            "encode_raster",
            "target raster encoder cannot preserve the embedded ICC profile",
        ));
    }

    let first_type = descriptor.sample_types()[0];
    if descriptor
        .sample_types()
        .iter()
        .any(|sample| *sample != first_type)
    {
        return Err(unsupported(
            "encode_raster",
            "target raster representation requires uniform channel precision",
        ));
    }
    let channel_count = names.len();
    let color_type = raster_color_type(channel_count, first_type, format)?;
    let sample_bytes = usize::from(first_type.bits() / 8);
    let byte_count = checked_product(
        &[
            u64::from(bounds.width()),
            u64::from(bounds.height()),
            channel_count as u64,
            sample_bytes as u64,
        ],
        "encode_raster",
    )?;
    let mut bytes = Vec::with_capacity(byte_count);
    for y in bounds.min_y()..bounds.max_y() {
        for x in bounds.min_x()..bounds.max_x() {
            for channel in 0..channel_count {
                let sample = storage.sample_bytes(channel, x, y).ok_or_else(|| {
                    internal(
                        "encode_raster",
                        "validated image storage is missing a sample",
                    )
                })?;
                bytes.extend_from_slice(sample);
            }
        }
    }
    Ok((bytes, color_type))
}

fn raster_color_type(
    channels: usize,
    sample_type: ImageSampleType,
    format: StillImageFormat,
) -> Result<ExtendedColorType> {
    let color_type = match (channels, sample_type) {
        (1, ImageSampleType::U8) => ExtendedColorType::L8,
        (2, ImageSampleType::U8) => ExtendedColorType::La8,
        (3, ImageSampleType::U8) => ExtendedColorType::Rgb8,
        (4, ImageSampleType::U8) => ExtendedColorType::Rgba8,
        (1, ImageSampleType::U16) => ExtendedColorType::L16,
        (2, ImageSampleType::U16) => ExtendedColorType::La16,
        (3, ImageSampleType::U16) => ExtendedColorType::Rgb16,
        (4, ImageSampleType::U16) => ExtendedColorType::Rgba16,
        (3, ImageSampleType::F32) => ExtendedColorType::Rgb32F,
        (4, ImageSampleType::F32) => ExtendedColorType::Rgba32F,
        _ => {
            return Err(unsupported(
                "encode_raster",
                "target raster representation cannot preserve this channel precision",
            ))
        }
    };
    let supported = match format {
        StillImageFormat::Png => matches!(
            color_type,
            ExtendedColorType::L8
                | ExtendedColorType::La8
                | ExtendedColorType::Rgb8
                | ExtendedColorType::Rgba8
                | ExtendedColorType::L16
                | ExtendedColorType::La16
                | ExtendedColorType::Rgb16
                | ExtendedColorType::Rgba16
        ),
        StillImageFormat::Tiff => matches!(
            color_type,
            ExtendedColorType::L8
                | ExtendedColorType::Rgb8
                | ExtendedColorType::Rgba8
                | ExtendedColorType::L16
                | ExtendedColorType::Rgb16
                | ExtendedColorType::Rgba16
                | ExtendedColorType::Rgb32F
                | ExtendedColorType::Rgba32F
        ),
        StillImageFormat::Jpeg => {
            matches!(color_type, ExtendedColorType::L8 | ExtendedColorType::Rgb8)
        }
        StillImageFormat::WebP => matches!(
            color_type,
            ExtendedColorType::L8
                | ExtendedColorType::La8
                | ExtendedColorType::Rgb8
                | ExtendedColorType::Rgba8
        ),
        StillImageFormat::Tga | StillImageFormat::Bmp => {
            matches!(
                color_type,
                ExtendedColorType::Rgb8 | ExtendedColorType::Rgba8
            )
        }
        StillImageFormat::Exr | StillImageFormat::Dpx => false,
    };
    if !supported {
        return Err(unsupported(
            "encode_raster",
            "target raster encoder cannot preserve this channel model and precision",
        )
        .with_context(format_context("representation", format)));
    }
    Ok(color_type)
}

struct RasterDescription {
    channels: &'static [&'static str],
    sample_type: ImageSampleType,
    sample_bytes: usize,
    has_alpha: bool,
}

impl RasterDescription {
    fn from_color_type(color_type: ColorType) -> Result<Self> {
        let (channels, sample_type, sample_bytes) = match color_type {
            ColorType::L8 => (&["Y"][..], ImageSampleType::U8, 1),
            ColorType::La8 => (&["Y", "A"][..], ImageSampleType::U8, 1),
            ColorType::Rgb8 => (&["R", "G", "B"][..], ImageSampleType::U8, 1),
            ColorType::Rgba8 => (&["R", "G", "B", "A"][..], ImageSampleType::U8, 1),
            ColorType::L16 => (&["Y"][..], ImageSampleType::U16, 2),
            ColorType::La16 => (&["Y", "A"][..], ImageSampleType::U16, 2),
            ColorType::Rgb16 => (&["R", "G", "B"][..], ImageSampleType::U16, 2),
            ColorType::Rgba16 => (&["R", "G", "B", "A"][..], ImageSampleType::U16, 2),
            ColorType::Rgb32F => (&["R", "G", "B"][..], ImageSampleType::F32, 4),
            ColorType::Rgba32F => (&["R", "G", "B", "A"][..], ImageSampleType::F32, 4),
            _ => {
                return Err(unsupported(
                    "decode_raster",
                    "decoder produced an unknown native channel representation",
                ))
            }
        };
        Ok(Self {
            channels,
            sample_type,
            sample_bytes,
            has_alpha: color_type.has_alpha(),
        })
    }
}

fn map_orientation(orientation: image::metadata::Orientation) -> Option<ImageOrientation> {
    use image::metadata::Orientation;
    match orientation {
        Orientation::NoTransforms => None,
        Orientation::FlipHorizontal => Some(ImageOrientation::TopRight),
        Orientation::Rotate180 => Some(ImageOrientation::BottomRight),
        Orientation::FlipVertical => Some(ImageOrientation::BottomLeft),
        Orientation::Rotate90FlipH => Some(ImageOrientation::LeftTop),
        Orientation::Rotate90 => Some(ImageOrientation::RightTop),
        Orientation::Rotate270FlipH => Some(ImageOrientation::RightBottom),
        Orientation::Rotate270 => Some(ImageOrientation::LeftBottom),
    }
}

fn read_exr<R: Read + Seek>(reader: &mut R, options: &ReadOptions) -> Result<StillImage> {
    use exr::image::read::read;
    use exr::prelude::{ReadChannels, ReadLayers};

    preflight_exr(reader, options)?;
    let original = read()
        .no_deep_data()
        .all_resolution_levels()
        .all_channels()
        .all_layers()
        .all_attributes()
        .non_parallel()
        .from_buffered(reader)
        .map_err(exr_read_error)?;
    let mut layers = Vec::with_capacity(original.layer_data.len());
    for layer in &original.layer_data {
        layers.push(exr_layer_to_access(&original.attributes, layer, options)?);
    }
    Ok(StillImage {
        layers,
        original: Some(OriginalRepresentation::Exr(Box::new(original))),
    })
}

fn write_exr<W: Write + Seek>(writer: &mut W, image: &StillImage) -> Result<()> {
    use exr::prelude::traits::WritableImage;

    if let Some(OriginalRepresentation::Exr(original)) = &image.original {
        return original
            .write()
            .non_parallel()
            .to_buffered(writer)
            .map_err(exr_write_error);
    }
    let exr = access_to_exr(image)?;
    exr.write()
        .non_parallel()
        .to_buffered(writer)
        .map_err(exr_write_error)
}

fn preflight_exr<R: Read + Seek>(reader: &mut R, options: &ReadOptions) -> Result<()> {
    use exr::meta::attribute::LevelMode;
    use exr::meta::BlockDescription;

    let start = reader
        .stream_position()
        .map_err(|source| file_error("inspect_exr", source))?;
    let metadata =
        exr::meta::MetaData::read_from_buffered(&mut *reader, true).map_err(exr_read_error)?;
    reader
        .seek(SeekFrom::Start(start))
        .map_err(|source| file_error("rewind_exr", source))?;
    let mut total_bytes = 0_u64;
    for header in &metadata.headers {
        if header.deep {
            return Err(unsupported(
                "decode_exr",
                "deep OpenEXR samples do not fit the flat ImageAccess contract",
            ));
        }
        let width = u32::try_from(header.layer_size.width())
            .map_err(|_| exhausted("decode_exr", "OpenEXR width exceeds the supported range"))?;
        let height = u32::try_from(header.layer_size.height())
            .map_err(|_| exhausted("decode_exr", "OpenEXR height exceeds the supported range"))?;
        if width > options.max_width || height > options.max_height {
            return Err(exhausted(
                "decode_exr",
                "OpenEXR dimensions exceed the configured limits",
            ));
        }
        if header
            .channels
            .list
            .iter()
            .any(|channel| channel.sampling != exr::math::Vec2(1, 1))
        {
            return Err(unsupported(
                "decode_exr",
                "subsampled OpenEXR channels do not fit ImageAccess channel geometry",
            ));
        }
        let level_sizes = match header.blocks {
            BlockDescription::ScanLines => vec![header.layer_size],
            BlockDescription::Tiles(description) => match description.level_mode {
                LevelMode::Singular => vec![header.layer_size],
                LevelMode::MipMap => {
                    exr::meta::mip_map_levels(description.rounding_mode, header.layer_size)
                        .map(|(_, size)| size)
                        .collect()
                }
                LevelMode::RipMap => {
                    return Err(unsupported(
                        "decode_exr",
                        "OpenEXR rip maps do not fit the single-axis ImageAccess mip contract",
                    ))
                }
            },
        };
        for size in level_sizes {
            for channel in &header.channels.list {
                let level_bytes = u64::try_from(size.area())
                    .ok()
                    .and_then(|area| {
                        area.checked_mul(channel.sample_type.bytes_per_sample() as u64)
                    })
                    .ok_or_else(|| exhausted("decode_exr", "OpenEXR decoded size overflows"))?;
                total_bytes = total_bytes
                    .checked_add(level_bytes)
                    .ok_or_else(|| exhausted("decode_exr", "OpenEXR decoded size overflows"))?;
                if total_bytes > options.max_decoded_bytes {
                    return Err(exhausted(
                        "decode_exr",
                        "OpenEXR decoded data exceeds the configured byte limit",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn exr_layer_to_access(
    image_attributes: &exr::meta::header::ImageAttributes,
    layer: &exr::image::Layer<exr::image::AnyChannels<exr::image::Levels<exr::image::FlatSamples>>>,
    options: &ReadOptions,
) -> Result<StillImageLayer> {
    use exr::image::{Blocks, Levels};

    let data_window = exr_bounds(layer.absolute_bounds())?;
    let display_window = exr_bounds(image_attributes.display_window)?;
    let names = layer
        .channel_data
        .list
        .iter()
        .map(|channel| channel.name.to_string())
        .collect::<Vec<_>>();
    let sample_types = layer
        .channel_data
        .list
        .iter()
        .map(|channel| exr_sample_type(&channel.sample_data))
        .collect::<Result<Vec<_>>>()?;
    let alpha_mode = if names.iter().any(|name| {
        name.rsplit('.')
            .next()
            .is_some_and(|component| component == "A")
    }) {
        AlphaMode::Premultiplied
    } else {
        AlphaMode::Opaque
    };
    let mut metadata = ImageMetadata::new();
    if image_attributes.pixel_aspect != 1.0 {
        metadata.insert(
            "exr.pixel_aspect_bits",
            ImageMetadataValue::Unsigned(u64::from(image_attributes.pixel_aspect.to_bits())),
        )?;
    }
    project_exr_attributes(&image_attributes.other, "exr.shared.", &mut metadata)?;
    project_exr_attributes(&layer.attributes.other, "exr.layer.", &mut metadata)?;
    let mut descriptor = ImageAccessDescriptor::new(
        data_window,
        display_window,
        ChannelList::from_full_names(names.iter().map(String::as_str))?,
        sample_types,
        ColorSpace::UNSPECIFIED,
        alpha_mode,
    )?
    .with_image_metadata(metadata);
    if let Some(position) = options.sequence_position {
        descriptor = descriptor.with_sequence_position(position);
    }

    let access = match layer.encoding.blocks {
        Blocks::ScanLines => {
            if layer
                .channel_data
                .list
                .iter()
                .any(|channel| !matches!(channel.sample_data, Levels::Singular(_)))
            {
                return Err(unsupported(
                    "decode_exr",
                    "scanline OpenEXR parts cannot contain resolution levels",
                ));
            }
            let samples = layer
                .channel_data
                .list
                .iter()
                .map(|channel| match &channel.sample_data {
                    Levels::Singular(samples) => Ok(samples),
                    _ => unreachable!("validated singular scanline samples"),
                })
                .collect::<Result<Vec<_>>>()?;
            ImageAccess::from_scanline(descriptor, exr_planar_storage(data_window, &samples)?)?
        }
        Blocks::Tiles(tile_size) => {
            let (mip_mode, rounding_mode, level_count) = exr_level_contract(layer)?;
            let tile_width = u32::try_from(tile_size.width()).map_err(|_| {
                exhausted(
                    "decode_exr",
                    "OpenEXR tile width exceeds the supported range",
                )
            })?;
            let tile_height = u32::try_from(tile_size.height()).map_err(|_| {
                exhausted(
                    "decode_exr",
                    "OpenEXR tile height exceeds the supported range",
                )
            })?;
            let description =
                TileDescription::new(tile_width, tile_height, mip_mode, rounding_mode)?;
            let mut tiles = Vec::new();
            for level_number in 0..level_count {
                let level = MipLevel::new(level_number as u32);
                let level_bounds = exr_level_bounds(data_window, layer, level_number)?;
                let columns = level_bounds.width().div_ceil(tile_width);
                let rows = level_bounds.height().div_ceil(tile_height);
                for tile_y in 0..rows {
                    for tile_x in 0..columns {
                        let tile_bounds = PixelBounds::new(
                            level_bounds.min_x()
                                + i32::try_from(tile_x * tile_width).expect("tile x fits i32"),
                            level_bounds.min_y()
                                + i32::try_from(tile_y * tile_height).expect("tile y fits i32"),
                            (level_bounds.min_x()
                                + i32::try_from((tile_x + 1) * tile_width)
                                    .expect("tile x fits i32"))
                            .min(level_bounds.max_x()),
                            (level_bounds.min_y()
                                + i32::try_from((tile_y + 1) * tile_height)
                                    .expect("tile y fits i32"))
                            .min(level_bounds.max_y()),
                        )?;
                        let level_samples = layer
                            .channel_data
                            .list
                            .iter()
                            .map(|channel| exr_level_samples(&channel.sample_data, level_number))
                            .collect::<Result<Vec<_>>>()?;
                        tiles.push(ImageTile::new(
                            level,
                            TileIndex::new(tile_x, tile_y),
                            exr_tile_storage(level_bounds, tile_bounds, &level_samples)?,
                        ));
                    }
                }
            }
            ImageAccess::tiled(descriptor, description, tiles)?
        }
    };
    StillImageLayer::new(
        layer
            .attributes
            .layer_name
            .as_ref()
            .map(ToString::to_string),
        access,
    )
}

fn exr_level_contract(
    layer: &exr::image::Layer<exr::image::AnyChannels<exr::image::Levels<exr::image::FlatSamples>>>,
) -> Result<(MipMode, LevelRoundingMode, usize)> {
    use exr::image::Levels;
    use exr::math::RoundingMode;

    let first = &layer
        .channel_data
        .list
        .first()
        .ok_or_else(|| invalid("decode_exr", "OpenEXR part has no channels"))?
        .sample_data;
    let contract = match first {
        Levels::Singular(_) => (MipMode::SingleLevel, LevelRoundingMode::Down, 1),
        Levels::Mip {
            rounding_mode,
            level_data,
        } => (
            MipMode::Mipmap,
            match rounding_mode {
                RoundingMode::Down => LevelRoundingMode::Down,
                RoundingMode::Up => LevelRoundingMode::Up,
            },
            level_data.len(),
        ),
        Levels::Rip { .. } => {
            return Err(unsupported(
                "decode_exr",
                "OpenEXR rip maps do not fit the single-axis ImageAccess mip contract",
            ))
        }
    };
    for channel in &layer.channel_data.list {
        let matches = match (&channel.sample_data, contract.0) {
            (Levels::Singular(_), MipMode::SingleLevel) => true,
            (Levels::Mip { level_data, .. }, MipMode::Mipmap) => level_data.len() == contract.2,
            _ => false,
        };
        if !matches {
            return Err(invalid(
                "decode_exr",
                "OpenEXR channels disagree about resolution levels",
            ));
        }
    }
    Ok(contract)
}

fn exr_level_bounds(
    data_window: PixelBounds,
    layer: &exr::image::Layer<exr::image::AnyChannels<exr::image::Levels<exr::image::FlatSamples>>>,
    level: usize,
) -> Result<PixelBounds> {
    let (_, rounding, _) = exr_level_contract(layer)?;
    let divisor = 1_u64.checked_shl(level as u32).ok_or_else(|| {
        exhausted(
            "decode_exr",
            "OpenEXR mip level exceeds the supported range",
        )
    })?;
    let dimension = |value: u32| -> u32 {
        match rounding {
            LevelRoundingMode::Down => (u64::from(value) / divisor).max(1) as u32,
            LevelRoundingMode::Up => u64::from(value).div_ceil(divisor).max(1) as u32,
        }
    };
    PixelBounds::from_origin_size(
        data_window.min_x(),
        data_window.min_y(),
        dimension(data_window.width()),
        dimension(data_window.height()),
    )
}

fn exr_level_samples(
    levels: &exr::image::Levels<exr::image::FlatSamples>,
    level: usize,
) -> Result<&exr::image::FlatSamples> {
    match levels {
        exr::image::Levels::Singular(samples) if level == 0 => Ok(samples),
        exr::image::Levels::Mip { level_data, .. } => level_data
            .get(level)
            .ok_or_else(|| invalid("decode_exr", "OpenEXR mip level is missing channel samples")),
        exr::image::Levels::Rip { .. } => Err(unsupported(
            "decode_exr",
            "OpenEXR rip maps do not fit ImageAccess",
        )),
        _ => Err(invalid(
            "decode_exr",
            "OpenEXR singular channel has an invalid level index",
        )),
    }
}

fn exr_planar_storage(
    bounds: PixelBounds,
    samples: &[&exr::image::FlatSamples],
) -> Result<ImageStorage> {
    let mut planes = Vec::with_capacity(samples.len());
    let mut slices = Vec::with_capacity(samples.len());
    for (index, samples) in samples.iter().enumerate() {
        let sample_bytes = exr_flat_sample_bytes(samples);
        let bytes = exr_flat_bytes(samples);
        let expected = checked_product(
            &[
                u64::from(bounds.width()),
                u64::from(bounds.height()),
                sample_bytes as u64,
            ],
            "decode_exr",
        )?;
        if bytes.len() != expected {
            return Err(invalid(
                "decode_exr",
                "OpenEXR channel sample count does not match its data window",
            ));
        }
        planes.push(StoragePlane::new(
            Arc::from(bytes),
            0,
            usize::try_from(bounds.width()).expect("u32 width fits usize") * sample_bytes,
            ByteAlignment::new(sample_bytes)?,
        )?);
        slices.push(ChannelSlice::new(index, 0, sample_bytes, sample_bytes)?);
    }
    ImageStorage::new(bounds, ChannelStorageLayout::Planar, planes, slices)
}

fn exr_tile_storage(
    level_bounds: PixelBounds,
    tile_bounds: PixelBounds,
    samples: &[&exr::image::FlatSamples],
) -> Result<ImageStorage> {
    let level_width = usize::try_from(level_bounds.width()).expect("u32 width fits usize");
    let mut planes = Vec::with_capacity(samples.len());
    let mut slices = Vec::with_capacity(samples.len());
    for (channel_index, samples) in samples.iter().enumerate() {
        let sample_bytes = exr_flat_sample_bytes(samples);
        let capacity = checked_product(
            &[
                u64::from(tile_bounds.width()),
                u64::from(tile_bounds.height()),
                sample_bytes as u64,
            ],
            "decode_exr",
        )?;
        let all_bytes = exr_flat_bytes(samples);
        let mut tile_bytes = Vec::with_capacity(capacity);
        for y in tile_bounds.min_y()..tile_bounds.max_y() {
            for x in tile_bounds.min_x()..tile_bounds.max_x() {
                let local_x = usize::try_from(x - level_bounds.min_x()).expect("local x fits");
                let local_y = usize::try_from(y - level_bounds.min_y()).expect("local y fits");
                let index = local_y
                    .checked_mul(level_width)
                    .and_then(|row| row.checked_add(local_x))
                    .and_then(|sample| sample.checked_mul(sample_bytes))
                    .ok_or_else(|| exhausted("decode_exr", "OpenEXR tile offset overflows"))?;
                let end = index.checked_add(sample_bytes).ok_or_else(|| {
                    exhausted("decode_exr", "OpenEXR tile sample range overflows")
                })?;
                tile_bytes.extend_from_slice(all_bytes.get(index..end).ok_or_else(|| {
                    invalid("decode_exr", "OpenEXR mip channel has too few samples")
                })?);
            }
        }
        planes.push(StoragePlane::new(
            Arc::from(tile_bytes),
            0,
            usize::try_from(tile_bounds.width()).expect("u32 width fits usize") * sample_bytes,
            ByteAlignment::new(sample_bytes)?,
        )?);
        slices.push(ChannelSlice::new(
            channel_index,
            0,
            sample_bytes,
            sample_bytes,
        )?);
    }
    ImageStorage::new(tile_bounds, ChannelStorageLayout::Planar, planes, slices)
}

fn exr_sample_type(
    levels: &exr::image::Levels<exr::image::FlatSamples>,
) -> Result<ImageSampleType> {
    exr_level_samples(levels, 0).map(|samples| match samples {
        exr::image::FlatSamples::F16(_) => ImageSampleType::F16,
        exr::image::FlatSamples::F32(_) => ImageSampleType::F32,
        exr::image::FlatSamples::U32(_) => ImageSampleType::U32,
    })
}

fn exr_flat_sample_bytes(samples: &exr::image::FlatSamples) -> usize {
    match samples {
        exr::image::FlatSamples::F16(_) => 2,
        exr::image::FlatSamples::F32(_) | exr::image::FlatSamples::U32(_) => 4,
    }
}

fn exr_flat_bytes(samples: &exr::image::FlatSamples) -> Vec<u8> {
    match samples {
        exr::image::FlatSamples::F16(values) => values
            .iter()
            .flat_map(|value| value.to_bits().to_ne_bytes())
            .collect(),
        exr::image::FlatSamples::F32(values) => values
            .iter()
            .flat_map(|value| value.to_bits().to_ne_bytes())
            .collect(),
        exr::image::FlatSamples::U32(values) => values
            .iter()
            .flat_map(|value| value.to_ne_bytes())
            .collect(),
    }
}

fn exr_bounds(bounds: exr::meta::attribute::IntegerBounds) -> Result<PixelBounds> {
    let width = u32::try_from(bounds.size.width())
        .map_err(|_| exhausted("decode_exr", "OpenEXR width exceeds u32"))?;
    let height = u32::try_from(bounds.size.height())
        .map_err(|_| exhausted("decode_exr", "OpenEXR height exceeds u32"))?;
    PixelBounds::from_origin_size(bounds.position.x(), bounds.position.y(), width, height)
}

fn project_exr_attributes(
    attributes: &std::collections::HashMap<
        exr::meta::attribute::Text,
        exr::meta::attribute::AttributeValue,
    >,
    prefix: &str,
    metadata: &mut ImageMetadata,
) -> Result<()> {
    use exr::meta::attribute::AttributeValue;

    let mut entries = attributes.iter().collect::<Vec<_>>();
    entries.sort_by_key(|(name, _)| name.as_slice());
    for (name, value) in entries {
        if prefix == "exr.layer." && name == "superiOrientation" {
            if let AttributeValue::I32(value) = value {
                let orientation = u8::try_from(*value)
                    .ok()
                    .and_then(|value| ImageOrientation::try_from(value).ok())
                    .ok_or_else(|| {
                        corrupt("decode_exr", "OpenEXR Superi orientation is invalid")
                    })?;
                metadata.set_orientation(orientation);
                continue;
            }
        }
        let name = name.to_string();
        let key = if let Some(encoded) = name.strip_prefix("superiMeta_") {
            decode_hex_key(encoded)?
        } else {
            format!("{prefix}{name}")
        };
        let value = match value {
            AttributeValue::Text(value) => ImageMetadataValue::Text(value.to_string()),
            AttributeValue::F64(value) => {
                ImageMetadataValue::Float(crate::metadata::ImageMetadataFloat::new(*value))
            }
            AttributeValue::F32(value) => ImageMetadataValue::Unsigned(u64::from(value.to_bits())),
            AttributeValue::I32(value) => ImageMetadataValue::Signed(i64::from(*value)),
            AttributeValue::Custom { kind, bytes } => match kind.to_string().as_str() {
                "superi_bool" if bytes.len() == 1 => ImageMetadataValue::Boolean(bytes[0] != 0),
                "superi_i64" if bytes.len() == 8 => ImageMetadataValue::Signed(i64::from_le_bytes(
                    bytes.as_slice().try_into().expect("length checked"),
                )),
                "superi_u64" if bytes.len() == 8 => ImageMetadataValue::Unsigned(
                    u64::from_le_bytes(bytes.as_slice().try_into().expect("length checked")),
                ),
                "superi_utf8" => {
                    ImageMetadataValue::Text(String::from_utf8(bytes.clone()).map_err(|_| {
                        corrupt("decode_exr", "OpenEXR Superi UTF-8 metadata is invalid")
                    })?)
                }
                _ => ImageMetadataValue::Bytes(Arc::from(bytes.clone())),
            },
            _ => continue,
        };
        metadata.insert(key, value)?;
    }
    Ok(())
}

fn access_to_exr(image: &StillImage) -> Result<exr::image::AnyImage> {
    use exr::image::{AnyChannel, AnyChannels, Blocks, Encoding, Image, Layer};
    use exr::meta::attribute::{Compression, IntegerBounds, LineOrder, Text};
    use exr::meta::header::ImageAttributes;
    use exr::prelude::SmallVec;

    let first = image
        .layers
        .first()
        .expect("validated still image has layers");
    let display = first.access.descriptor().display_window();
    let display_bounds = IntegerBounds::new(
        exr::math::Vec2(display.min_x(), display.min_y()),
        exr::math::Vec2(
            usize::try_from(display.width()).expect("u32 width fits usize"),
            usize::try_from(display.height()).expect("u32 height fits usize"),
        ),
    );
    let mut image_attributes = ImageAttributes::new(display_bounds);
    image_attributes.pixel_aspect = exr_pixel_aspect(first.access.descriptor().metadata())?;
    let mut layers = SmallVec::new();
    for source_layer in &image.layers {
        let access = source_layer.access();
        if access.descriptor().display_window() != display {
            return Err(unsupported(
                "encode_exr",
                "OpenEXR multipart output requires one shared display window",
            ));
        }
        if exr_pixel_aspect(access.descriptor().metadata())? != image_attributes.pixel_aspect {
            return Err(unsupported(
                "encode_exr",
                "OpenEXR multipart output requires one shared pixel aspect ratio",
            ));
        }
        if access.descriptor().color_space() != ColorSpace::UNSPECIFIED
            || access.descriptor().color_tags().named_space().is_some()
            || access.descriptor().color_tags().icc_profile().is_some()
        {
            return Err(unsupported(
                "encode_exr",
                "OpenEXR output requires explicit chromaticities before preserving a color declaration",
            ));
        }
        let has_alpha = access
            .descriptor()
            .channels()
            .iter()
            .any(|name| name.base_name() == "A");
        if (has_alpha && access.descriptor().alpha_mode() != AlphaMode::Premultiplied)
            || (!has_alpha && access.descriptor().alpha_mode() != AlphaMode::Opaque)
        {
            return Err(unsupported(
                "encode_exr",
                "OpenEXR output follows premultiplied alpha convention",
            ));
        }
        let mut channels = SmallVec::new();
        for (channel_index, channel_name) in access.descriptor().channels().iter().enumerate() {
            let text = Text::new_or_none(channel_name.as_str()).ok_or_else(|| {
                unsupported(
                    "encode_exr",
                    "OpenEXR channel names must contain only byte-representable characters",
                )
            })?;
            channels.push(AnyChannel::new(
                text,
                access_channel_to_exr_levels(access, ChannelIndex::new(channel_index))?,
            ));
        }
        let channels = AnyChannels::sort(channels);
        let mut attributes = match source_layer.name() {
            Some(name) => exr::meta::header::LayerAttributes::named(
                Text::new_or_none(name).ok_or_else(|| {
                    unsupported(
                        "encode_exr",
                        "OpenEXR layer names must contain only byte-representable characters",
                    )
                })?,
            ),
            None => exr::meta::header::LayerAttributes::default(),
        };
        attributes.layer_position = exr::math::Vec2(
            access.descriptor().data_window().min_x(),
            access.descriptor().data_window().min_y(),
        );
        populate_exr_metadata(
            access.descriptor().metadata(),
            &mut image_attributes.other,
            &mut attributes.other,
        )?;
        let encoding = match access.organization() {
            ImageOrganization::Scanline => Encoding::SMALL_LOSSLESS,
            ImageOrganization::Tiled => {
                let tiles = access
                    .tile_description()
                    .expect("tiled access has description");
                Encoding {
                    compression: Compression::ZIP16,
                    blocks: Blocks::Tiles(exr::math::Vec2(
                        tiles.width() as usize,
                        tiles.height() as usize,
                    )),
                    line_order: LineOrder::Unspecified,
                }
            }
        };
        layers.push(Layer::new(
            exr::math::Vec2(
                access.descriptor().data_window().width() as usize,
                access.descriptor().data_window().height() as usize,
            ),
            attributes,
            encoding,
            channels,
        ));
    }
    Ok(Image::from_layers(image_attributes, layers))
}

fn populate_exr_metadata(
    metadata: &ImageMetadata,
    shared: &mut std::collections::HashMap<
        exr::meta::attribute::Text,
        exr::meta::attribute::AttributeValue,
    >,
    layer: &mut std::collections::HashMap<
        exr::meta::attribute::Text,
        exr::meta::attribute::AttributeValue,
    >,
) -> Result<()> {
    use exr::meta::attribute::{AttributeValue, Text};

    if metadata.timecode().is_some() {
        return Err(unsupported(
            "encode_exr",
            "typed timecode requires an explicit OpenEXR timecode mapping",
        ));
    }
    if let Some(orientation) = metadata.orientation() {
        layer.insert(
            Text::new_or_panic("superiOrientation"),
            AttributeValue::I32(i32::from(orientation.exif_value())),
        );
    }
    for (key, value) in metadata.iter() {
        if key == "exr.pixel_aspect_bits" {
            continue;
        }
        let (target, attribute_name) = if let Some(name) = key.strip_prefix("exr.shared.") {
            (true, exr_text(name, "OpenEXR shared metadata key")?)
        } else if let Some(name) = key.strip_prefix("exr.layer.") {
            (false, exr_text(name, "OpenEXR layer metadata key")?)
        } else {
            let encoded = format!("superiMeta_{}", encode_hex_key(key));
            (false, Text::new_or_panic(encoded))
        };
        let attribute_value = image_metadata_to_exr(value)?;
        let destination = if target { &mut *shared } else { &mut *layer };
        if let Some(existing) = destination.get(&attribute_name) {
            if existing != &attribute_value {
                return Err(unsupported(
                    "encode_exr",
                    "OpenEXR multipart layers disagree about shared metadata",
                ));
            }
        } else {
            destination.insert(attribute_name, attribute_value);
        }
    }
    Ok(())
}

fn image_metadata_to_exr(
    value: &ImageMetadataValue,
) -> Result<exr::meta::attribute::AttributeValue> {
    use exr::meta::attribute::{AttributeValue, Text};

    Ok(match value {
        ImageMetadataValue::Boolean(value) => AttributeValue::Custom {
            kind: Text::new_or_panic("superi_bool"),
            bytes: vec![u8::from(*value)],
        },
        ImageMetadataValue::Text(value) => match Text::new_or_none(value) {
            Some(value) => AttributeValue::Text(value),
            None => AttributeValue::Custom {
                kind: Text::new_or_panic("superi_utf8"),
                bytes: value.as_bytes().to_vec(),
            },
        },
        ImageMetadataValue::Signed(value) => AttributeValue::Custom {
            kind: Text::new_or_panic("superi_i64"),
            bytes: value.to_le_bytes().to_vec(),
        },
        ImageMetadataValue::Unsigned(value) => AttributeValue::Custom {
            kind: Text::new_or_panic("superi_u64"),
            bytes: value.to_le_bytes().to_vec(),
        },
        ImageMetadataValue::Float(value) => AttributeValue::F64(value.value()),
        ImageMetadataValue::Bytes(value) => AttributeValue::Custom {
            kind: Text::new_or_panic("superi_bytes"),
            bytes: value.to_vec(),
        },
    })
}

fn exr_text(value: &str, field: &'static str) -> Result<exr::meta::attribute::Text> {
    exr::meta::attribute::Text::new_or_none(value).ok_or_else(|| {
        unsupported(
            "encode_exr",
            format!("{field} must contain only byte-representable characters"),
        )
    })
}

fn encode_hex_key(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn decode_hex_key(value: &str) -> Result<String> {
    if value.len() % 2 != 0 {
        return Err(corrupt(
            "decode_exr",
            "OpenEXR Superi metadata key has invalid hex length",
        ));
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = hex_value(pair[0])
            .ok_or_else(|| corrupt("decode_exr", "OpenEXR Superi metadata key is not hex"))?;
        let low = hex_value(pair[1])
            .ok_or_else(|| corrupt("decode_exr", "OpenEXR Superi metadata key is not hex"))?;
        bytes.push((high << 4) | low);
    }
    String::from_utf8(bytes)
        .map_err(|_| corrupt("decode_exr", "OpenEXR Superi metadata key is not UTF-8"))
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn access_channel_to_exr_levels(
    access: &ImageAccess,
    channel: ChannelIndex,
) -> Result<exr::image::Levels<exr::image::FlatSamples>> {
    use exr::image::Levels;
    use exr::math::RoundingMode;

    let values = access
        .levels()
        .map(|level| access_channel_to_exr_samples(access, channel, level))
        .collect::<Result<Vec<_>>>()?;
    let Some(description) = access.tile_description() else {
        return Ok(Levels::Singular(
            values
                .into_iter()
                .next()
                .expect("scanline image has one level"),
        ));
    };
    match description.mip_mode() {
        MipMode::SingleLevel => Ok(Levels::Singular(
            values
                .into_iter()
                .next()
                .expect("single level image has data"),
        )),
        MipMode::Mipmap => Ok(Levels::Mip {
            rounding_mode: match description.rounding_mode() {
                LevelRoundingMode::Down => RoundingMode::Down,
                LevelRoundingMode::Up => RoundingMode::Up,
            },
            level_data: values,
        }),
    }
}

fn access_channel_to_exr_samples(
    access: &ImageAccess,
    channel: ChannelIndex,
    level: MipLevel,
) -> Result<exr::image::FlatSamples> {
    let bounds = access.level_bounds(level)?;
    let region = access.region(level, bounds, &[channel])?;
    let sample_type = access
        .descriptor()
        .sample_type(channel)
        .expect("validated channel index exists");
    match sample_type {
        ImageSampleType::F16 => {
            let mut values = Vec::with_capacity(checked_product(
                &[u64::from(bounds.width()), u64::from(bounds.height())],
                "encode_exr",
            )?);
            for y in bounds.min_y()..bounds.max_y() {
                for x in bounds.min_x()..bounds.max_x() {
                    let bytes = region.sample_bytes(channel, x, y).ok_or_else(|| {
                        internal("encode_exr", "validated access is missing a channel sample")
                    })?;
                    values.push(exr::prelude::f16::from_bits(u16::from_ne_bytes([
                        bytes[0], bytes[1],
                    ])));
                }
            }
            Ok(exr::image::FlatSamples::F16(values))
        }
        ImageSampleType::F32 => {
            let mut values = Vec::with_capacity(checked_product(
                &[u64::from(bounds.width()), u64::from(bounds.height())],
                "encode_exr",
            )?);
            for y in bounds.min_y()..bounds.max_y() {
                for x in bounds.min_x()..bounds.max_x() {
                    let bytes = region.sample_bytes(channel, x, y).ok_or_else(|| {
                        internal("encode_exr", "validated access is missing a channel sample")
                    })?;
                    values.push(f32::from_bits(u32::from_ne_bytes([
                        bytes[0], bytes[1], bytes[2], bytes[3],
                    ])));
                }
            }
            Ok(exr::image::FlatSamples::F32(values))
        }
        ImageSampleType::U32 => {
            let mut values = Vec::with_capacity(checked_product(
                &[u64::from(bounds.width()), u64::from(bounds.height())],
                "encode_exr",
            )?);
            for y in bounds.min_y()..bounds.max_y() {
                for x in bounds.min_x()..bounds.max_x() {
                    let bytes = region.sample_bytes(channel, x, y).ok_or_else(|| {
                        internal("encode_exr", "validated access is missing a channel sample")
                    })?;
                    values.push(u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
                }
            }
            Ok(exr::image::FlatSamples::U32(values))
        }
        ImageSampleType::U8 | ImageSampleType::U16 => Err(unsupported(
            "encode_exr",
            "OpenEXR cannot preserve U8 or U16 as a native channel representation",
        )),
    }
}

fn exr_pixel_aspect(metadata: &ImageMetadata) -> Result<f32> {
    if let Some(ImageMetadataValue::Unsigned(bits)) = metadata.get("exr.pixel_aspect_bits") {
        let bits = u32::try_from(*bits)
            .map_err(|_| invalid("encode_exr", "OpenEXR pixel aspect bits must fit u32"))?;
        let value = f32::from_bits(bits);
        if !value.is_finite() || value <= 0.0 {
            return Err(invalid(
                "encode_exr",
                "OpenEXR pixel aspect ratio must be finite and positive",
            ));
        }
        return Ok(value);
    }
    if let Some(ratio) = metadata.pixel_aspect_ratio() {
        return Ok(ratio.numerator() as f32 / ratio.denominator() as f32);
    }
    Ok(1.0)
}

fn exr_read_error(source: exr::error::Error) -> Error {
    let category = match source {
        exr::error::Error::NotSupported(_) => ErrorCategory::Unsupported,
        exr::error::Error::Io(_) => ErrorCategory::Unavailable,
        exr::error::Error::Aborted => ErrorCategory::Cancelled,
        exr::error::Error::Invalid(_) => ErrorCategory::CorruptData,
    };
    Error::with_source(
        category,
        Recoverability::UserCorrectable,
        "OpenEXR decoder rejected the input",
        source,
    )
    .with_context(format_context("decode_exr", StillImageFormat::Exr))
}

fn exr_write_error(source: exr::error::Error) -> Error {
    let category = match source {
        exr::error::Error::NotSupported(_) => ErrorCategory::Unsupported,
        exr::error::Error::Io(_) => ErrorCategory::Unavailable,
        exr::error::Error::Aborted => ErrorCategory::Cancelled,
        exr::error::Error::Invalid(_) => ErrorCategory::InvalidInput,
    };
    Error::with_source(
        category,
        Recoverability::UserCorrectable,
        "OpenEXR encoder rejected the image",
        source,
    )
    .with_context(format_context("encode_exr", StillImageFormat::Exr))
}

fn read_dpx<R: Read + Seek>(reader: &mut R, options: &ReadOptions) -> Result<StillImage> {
    const MAX_HEADER_OVERHEAD: u64 = 16 * 1024 * 1024;

    let start = reader
        .stream_position()
        .map_err(|source| file_error("inspect_dpx", source))?;
    let mut header = [0_u8; 2048];
    reader
        .read_exact(&mut header)
        .map_err(|source| dpx_io_error("read_dpx_header", source))?;
    let endianness = match &header[..4] {
        b"SDPX" => DpxEndianness::Big,
        b"XPDS" => DpxEndianness::Little,
        _ => return Err(corrupt("decode_dpx", "DPX magic number is missing")),
    };
    let read_u16 = |offset| dpx_u16(&header, offset, endianness);
    let read_u32 = |offset| dpx_u32(&header, offset, endianness);
    let width = read_u32(772)?;
    let height = read_u32(776)?;
    if width == 0 || height == 0 || width == u32::MAX || height == u32::MAX {
        return Err(corrupt(
            "decode_dpx",
            "DPX image dimensions must be declared and nonzero",
        ));
    }
    if width > options.max_width || height > options.max_height {
        return Err(exhausted(
            "decode_dpx",
            "DPX dimensions exceed the configured limits",
        ));
    }
    let orientation = read_u16(768)?;
    if orientation > 3 {
        return Err(unsupported(
            "decode_dpx",
            "axis-swapped DPX orientation does not fit the current storage contract",
        ));
    }
    let element_count = read_u16(770)?;
    if element_count != 1 {
        return Err(unsupported(
            "decode_dpx",
            "this DPX adapter requires one combined image element",
        ));
    }
    let descriptor = header[800];
    let (file_channels, logical_channels, channel_map): (&[&str], &[&str], &[usize]) =
        match descriptor {
            6 => (&["Y"], &["Y"], &[0]),
            50 => (&["R", "G", "B"], &["R", "G", "B"], &[0, 1, 2]),
            51 => (&["R", "G", "B", "A"], &["R", "G", "B", "A"], &[0, 1, 2, 3]),
            52 => (&["A", "B", "G", "R"], &["R", "G", "B", "A"], &[3, 2, 1, 0]),
            _ => {
                return Err(unsupported(
                    "decode_dpx",
                    "DPX element descriptor is not luma, RGB, RGBA, or ABGR",
                ))
            }
        };
    let bit_depth = header[803];
    if !matches!(bit_depth, 8 | 10 | 12 | 16) {
        return Err(unsupported(
            "decode_dpx",
            "DPX bit depth is not 8, 10, 12, or 16",
        ));
    }
    let packing_code = read_u16(804)?;
    let packing = match (bit_depth, packing_code) {
        (8 | 16, 0) => DpxPacking::FilledMethodA,
        (10 | 12, 1) => DpxPacking::FilledMethodA,
        (10 | 12, 2) => DpxPacking::FilledMethodB,
        _ => {
            return Err(unsupported(
                "decode_dpx",
                "DPX packed or nonstandard component packing is not supported",
            ))
        }
    };
    if read_u16(806)? != 0 {
        return Err(unsupported(
            "decode_dpx",
            "run-length encoded DPX image elements are not supported",
        ));
    }
    let data_offset = match read_u32(808)? {
        0 | u32::MAX => read_u32(4)?,
        offset => offset,
    };
    if data_offset < 2048 {
        return Err(corrupt(
            "decode_dpx",
            "DPX image data offset overlaps required headers",
        ));
    }
    let eol_padding = match read_u32(812)? {
        u32::MAX => 0,
        value => value,
    };
    let eoi_padding = match read_u32(816)? {
        u32::MAX => 0,
        value => value,
    };
    let samples_per_row = u64::from(width)
        .checked_mul(file_channels.len() as u64)
        .ok_or_else(|| exhausted("decode_dpx", "DPX row sample count overflows"))?;
    let row_bytes = dpx_row_bytes(samples_per_row, bit_depth)?;
    let image_bytes = (row_bytes + u64::from(eol_padding))
        .checked_mul(u64::from(height))
        .and_then(|bytes| bytes.checked_add(u64::from(eoi_padding)))
        .ok_or_else(|| exhausted("decode_dpx", "DPX image byte count overflows"))?;
    let decoded_bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(logical_channels.len() as u64))
        .and_then(|samples| samples.checked_mul(if bit_depth == 8 { 1 } else { 2 }))
        .ok_or_else(|| exhausted("decode_dpx", "DPX decoded byte count overflows"))?;
    if decoded_bytes > options.max_decoded_bytes {
        return Err(exhausted(
            "decode_dpx",
            "DPX decoded data exceeds the configured byte limit",
        ));
    }
    let required_end = u64::from(data_offset)
        .checked_add(image_bytes)
        .ok_or_else(|| exhausted("decode_dpx", "DPX data range overflows"))?;
    let max_file_bytes = options
        .max_decoded_bytes
        .checked_add(MAX_HEADER_OVERHEAD)
        .ok_or_else(|| exhausted("decode_dpx", "DPX file limit overflows"))?;
    if required_end > max_file_bytes {
        return Err(exhausted(
            "decode_dpx",
            "DPX header and image data exceed the configured file limit",
        ));
    }
    reader
        .seek(SeekFrom::Start(start))
        .map_err(|source| file_error("rewind_dpx", source))?;
    let mut bytes = Vec::new();
    reader
        .take(max_file_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| dpx_io_error("read_dpx", source))?;
    if bytes.len() as u64 > max_file_bytes {
        return Err(exhausted(
            "decode_dpx",
            "DPX file exceeds the configured file limit",
        ));
    }
    if (bytes.len() as u64) < required_end {
        return Err(corrupt(
            "decode_dpx",
            "DPX image data ends before the declared element range",
        ));
    }

    let sample_bytes = if bit_depth == 8 { 1 } else { 2 };
    let plane_capacity = checked_product(
        &[u64::from(width), u64::from(height), sample_bytes as u64],
        "decode_dpx",
    )?;
    let mut planes = (0..logical_channels.len())
        .map(|_| Vec::with_capacity(plane_capacity))
        .collect::<Vec<_>>();
    let mut row_start = data_offset as usize;
    for _ in 0..height {
        let row_end = row_start
            .checked_add(usize::try_from(row_bytes).map_err(|_| {
                exhausted("decode_dpx", "DPX row size exceeds the host address space")
            })?)
            .ok_or_else(|| exhausted("decode_dpx", "DPX row range overflows"))?;
        let row = bytes
            .get(row_start..row_end)
            .ok_or_else(|| corrupt("decode_dpx", "DPX row lies outside the image data range"))?;
        let samples = decode_dpx_row(
            row,
            usize::try_from(samples_per_row)
                .map_err(|_| exhausted("decode_dpx", "DPX row sample count exceeds the host"))?,
            bit_depth,
            packing,
            endianness,
        )?;
        for pixel in samples.chunks_exact(file_channels.len()) {
            for (logical, file_index) in channel_map.iter().copied().enumerate() {
                if bit_depth == 8 {
                    planes[logical].push(pixel[file_index] as u8);
                } else {
                    planes[logical].extend_from_slice(&pixel[file_index].to_ne_bytes());
                }
            }
        }
        row_start = row_end
            .checked_add(eol_padding as usize)
            .ok_or_else(|| exhausted("decode_dpx", "DPX row padding overflows"))?;
    }

    let source_x = read_u32(1408)?;
    let source_y = read_u32(1412)?;
    let min_x = if source_x == u32::MAX { 0 } else { source_x };
    let min_y = if source_y == u32::MAX { 0 } else { source_y };
    let min_x = i32::try_from(min_x)
        .map_err(|_| unsupported("decode_dpx", "DPX source x offset exceeds i32"))?;
    let min_y = i32::try_from(min_y)
        .map_err(|_| unsupported("decode_dpx", "DPX source y offset exceeds i32"))?;
    let data_window = PixelBounds::from_origin_size(min_x, min_y, width, height)?;
    let original_width = read_u32(1424)?;
    let original_height = read_u32(1428)?;
    let display_window = if original_width == 0
        || original_height == 0
        || original_width == u32::MAX
        || original_height == u32::MAX
    {
        data_window
    } else {
        PixelBounds::from_origin_size(0, 0, original_width, original_height)?
    };
    let mut metadata = ImageMetadata::new().with_orientation(match orientation {
        0 => ImageOrientation::TopLeft,
        1 => ImageOrientation::TopRight,
        2 => ImageOrientation::BottomLeft,
        3 => ImageOrientation::BottomRight,
        _ => unreachable!("orientation was validated"),
    });
    let aspect_numerator = read_u32(1628)?;
    let aspect_denominator = read_u32(1632)?;
    if !matches!(aspect_numerator, 0 | u32::MAX) && !matches!(aspect_denominator, 0 | u32::MAX) {
        metadata.set_pixel_aspect_ratio(superi_core::geometry::AspectRatio::new(
            aspect_numerator,
            aspect_denominator,
        )?);
    }
    metadata.insert(
        "dpx.transfer_characteristic",
        ImageMetadataValue::Unsigned(u64::from(header[801])),
    )?;
    metadata.insert(
        "dpx.colorimetric_specification",
        ImageMetadataValue::Unsigned(u64::from(header[802])),
    )?;
    metadata.insert(
        "dpx.bit_depth",
        ImageMetadataValue::Unsigned(u64::from(bit_depth)),
    )?;
    let frame_position = read_u32(1712)?;
    if frame_position != u32::MAX {
        metadata.insert(
            "dpx.frame_position",
            ImageMetadataValue::Unsigned(u64::from(frame_position)),
        )?;
    }
    let sequence_length = read_u32(1716)?;
    if sequence_length != u32::MAX {
        metadata.insert(
            "dpx.sequence_length",
            ImageMetadataValue::Unsigned(u64::from(sequence_length)),
        )?;
    }
    let timecode = read_u32(1920)?;
    if timecode != u32::MAX {
        metadata.insert(
            "dpx.timecode_bits",
            ImageMetadataValue::Unsigned(u64::from(timecode)),
        )?;
    }

    let sample_type = if bit_depth == 8 {
        ImageSampleType::U8
    } else {
        ImageSampleType::U16
    };
    let mut storage_planes = Vec::with_capacity(planes.len());
    let mut slices = Vec::with_capacity(planes.len());
    for (index, plane) in planes.into_iter().enumerate() {
        storage_planes.push(StoragePlane::new(
            Arc::from(plane),
            0,
            usize::try_from(width).expect("u32 width fits usize") * sample_bytes,
            ByteAlignment::new(sample_bytes)?,
        )?);
        slices.push(ChannelSlice::new(index, 0, sample_bytes, sample_bytes)?);
    }
    let storage = ImageStorage::new(
        data_window,
        ChannelStorageLayout::Planar,
        storage_planes,
        slices,
    )?;
    let mut descriptor = ImageAccessDescriptor::new(
        data_window,
        display_window,
        ChannelList::from_full_names(logical_channels.iter().copied())?,
        vec![sample_type; logical_channels.len()],
        ColorSpace::UNSPECIFIED,
        if logical_channels.contains(&"A") {
            AlphaMode::Straight
        } else {
            AlphaMode::Opaque
        },
    )?
    .with_image_metadata(metadata);
    if let Some(position) = options.sequence_position {
        descriptor = descriptor.with_sequence_position(position);
    }
    let layer = StillImageLayer::new(None, ImageAccess::from_scanline(descriptor, storage)?)?;
    Ok(StillImage {
        layers: vec![layer],
        original: Some(OriginalRepresentation::Dpx {
            bytes: Arc::from(bytes),
            options: WriteOptions {
                jpeg_quality: WriteOptions::default().jpeg_quality,
                dpx_endianness: endianness,
                dpx_packing: packing,
                dpx_bit_depth: bit_depth,
            },
        }),
    })
}

fn write_dpx<W: Write + Seek>(
    writer: &mut W,
    image: &StillImage,
    options: &WriteOptions,
) -> Result<()> {
    if let Some(OriginalRepresentation::Dpx {
        bytes,
        options: source_options,
    }) = &image.original
    {
        if options == source_options {
            writer
                .write_all(bytes)
                .map_err(|source| dpx_io_error("write_dpx", source))?;
            return Ok(());
        }
    }
    let access = image.single_access()?;
    let descriptor = access.descriptor();
    let data_window = descriptor.data_window();
    let display_window = descriptor.display_window();
    if access.organization() != ImageOrganization::Scanline || access.level_count() != 1 {
        return Err(unsupported(
            "encode_dpx",
            "DPX output cannot preserve tiled or mipmapped storage",
        ));
    }
    if data_window.min_x() < 0 || data_window.min_y() < 0 {
        return Err(unsupported(
            "encode_dpx",
            "DPX source offsets cannot preserve negative data-window origins",
        ));
    }
    if display_window.min_x() != 0 || display_window.min_y() != 0 {
        return Err(unsupported(
            "encode_dpx",
            "DPX original dimensions cannot preserve a nonzero display origin",
        ));
    }
    if !display_window.contains(data_window.min_x(), data_window.min_y())
        || !display_window.contains(data_window.max_x() - 1, data_window.max_y() - 1)
    {
        return Err(invalid(
            "encode_dpx",
            "DPX data window must lie within the original display dimensions",
        ));
    }
    let names = descriptor
        .channels()
        .iter()
        .map(|name| name.as_str())
        .collect::<Vec<_>>();
    let (element_descriptor, description) = match names.as_slice() {
        ["Y"] => (6_u8, "Luminance"),
        ["R", "G", "B"] => (50_u8, "RGB"),
        ["R", "G", "B", "A"] => (51_u8, "RGBA"),
        _ => {
            return Err(unsupported(
                "encode_dpx",
                "DPX output requires Y, RGB, or RGBA channels in standard order",
            ))
        }
    };
    let has_alpha = element_descriptor == 51;
    if (has_alpha && descriptor.alpha_mode() != AlphaMode::Straight)
        || (!has_alpha && descriptor.alpha_mode() != AlphaMode::Opaque)
    {
        return Err(unsupported(
            "encode_dpx",
            "DPX RGBA output requires straight alpha and non-alpha output requires opaque interpretation",
        ));
    }
    if descriptor.color_space() != ColorSpace::UNSPECIFIED
        || descriptor.color_tags().named_space().is_some()
        || descriptor.color_tags().icc_profile().is_some()
    {
        return Err(unsupported(
            "encode_dpx",
            "DPX output requires transfer and colorimetric metadata for declared color meaning",
        ));
    }
    let expected_type = if options.dpx_bit_depth == 8 {
        ImageSampleType::U8
    } else {
        ImageSampleType::U16
    };
    if descriptor
        .sample_types()
        .iter()
        .any(|sample_type| *sample_type != expected_type)
    {
        return Err(unsupported(
            "encode_dpx",
            "DPX source precision must match the selected integer bit depth",
        ));
    }
    let width = data_window.width();
    let height = data_window.height();
    let samples_per_row = u64::from(width)
        .checked_mul(names.len() as u64)
        .ok_or_else(|| exhausted("encode_dpx", "DPX row sample count overflows"))?;
    let row_bytes = dpx_row_bytes(samples_per_row, options.dpx_bit_depth)?;
    let image_bytes = row_bytes
        .checked_mul(u64::from(height))
        .ok_or_else(|| exhausted("encode_dpx", "DPX image byte count overflows"))?;
    let file_size = 2048_u64
        .checked_add(image_bytes)
        .ok_or_else(|| exhausted("encode_dpx", "DPX file size overflows"))?;
    let file_size_u32 = u32::try_from(file_size)
        .map_err(|_| exhausted("encode_dpx", "DPX file exceeds its 32-bit size field"))?;
    let mut header = vec![0xff_u8; 2048];
    match options.dpx_endianness {
        DpxEndianness::Big => header[..4].copy_from_slice(b"SDPX"),
        DpxEndianness::Little => header[..4].copy_from_slice(b"XPDS"),
    }
    put_dpx_u32(&mut header, 4, 2048, options.dpx_endianness)?;
    header[8..16].copy_from_slice(b"V2.0\0\0\0\0");
    put_dpx_u32(&mut header, 16, file_size_u32, options.dpx_endianness)?;
    put_dpx_u32(&mut header, 24, 1664, options.dpx_endianness)?;
    put_dpx_u32(&mut header, 28, 384, options.dpx_endianness)?;
    put_dpx_u32(&mut header, 32, 0, options.dpx_endianness)?;
    put_dpx_u16(
        &mut header,
        768,
        dpx_orientation_code(descriptor.metadata().orientation())?,
        options.dpx_endianness,
    )?;
    put_dpx_u16(&mut header, 770, 1, options.dpx_endianness)?;
    put_dpx_u32(&mut header, 772, width, options.dpx_endianness)?;
    put_dpx_u32(&mut header, 776, height, options.dpx_endianness)?;
    put_dpx_u32(&mut header, 780, 0, options.dpx_endianness)?;
    put_dpx_u32(&mut header, 784, 0, options.dpx_endianness)?;
    put_dpx_u32(&mut header, 788, 0, options.dpx_endianness)?;
    let maximum = (1_u32 << options.dpx_bit_depth) - 1;
    put_dpx_u32(&mut header, 792, maximum, options.dpx_endianness)?;
    put_dpx_u32(&mut header, 796, 1.0_f32.to_bits(), options.dpx_endianness)?;
    header[800] = element_descriptor;
    header[801] = dpx_metadata_u8(descriptor.metadata(), "dpx.transfer_characteristic", 0)?;
    header[802] = dpx_metadata_u8(descriptor.metadata(), "dpx.colorimetric_specification", 0)?;
    header[803] = options.dpx_bit_depth;
    put_dpx_u16(
        &mut header,
        804,
        if matches!(options.dpx_bit_depth, 10 | 12) {
            match options.dpx_packing {
                DpxPacking::FilledMethodA => 1,
                DpxPacking::FilledMethodB => 2,
            }
        } else {
            0
        },
        options.dpx_endianness,
    )?;
    put_dpx_u16(&mut header, 806, 0, options.dpx_endianness)?;
    put_dpx_u32(&mut header, 808, 2048, options.dpx_endianness)?;
    put_dpx_u32(&mut header, 812, 0, options.dpx_endianness)?;
    put_dpx_u32(&mut header, 816, 0, options.dpx_endianness)?;
    let description_bytes = description.as_bytes();
    header[820..820 + description_bytes.len()].copy_from_slice(description_bytes);
    put_dpx_u32(
        &mut header,
        1408,
        data_window.min_x() as u32,
        options.dpx_endianness,
    )?;
    put_dpx_u32(
        &mut header,
        1412,
        data_window.min_y() as u32,
        options.dpx_endianness,
    )?;
    put_dpx_u32(
        &mut header,
        1424,
        display_window.width(),
        options.dpx_endianness,
    )?;
    put_dpx_u32(
        &mut header,
        1428,
        display_window.height(),
        options.dpx_endianness,
    )?;
    if let Some(aspect) = descriptor.metadata().pixel_aspect_ratio() {
        put_dpx_u32(
            &mut header,
            1628,
            aspect.numerator(),
            options.dpx_endianness,
        )?;
        put_dpx_u32(
            &mut header,
            1632,
            aspect.denominator(),
            options.dpx_endianness,
        )?;
    }
    if let Some(value) = dpx_metadata_u32(descriptor.metadata(), "dpx.frame_position")? {
        put_dpx_u32(&mut header, 1712, value, options.dpx_endianness)?;
    }
    if let Some(value) = dpx_metadata_u32(descriptor.metadata(), "dpx.sequence_length")? {
        put_dpx_u32(&mut header, 1716, value, options.dpx_endianness)?;
    }
    if let Some(value) = dpx_metadata_u32(descriptor.metadata(), "dpx.timecode_bits")? {
        put_dpx_u32(&mut header, 1920, value, options.dpx_endianness)?;
    }
    writer
        .write_all(&header)
        .map_err(|source| dpx_io_error("write_dpx_header", source))?;
    let region = access.region_all(MipLevel::BASE, data_window)?;
    for y in data_window.min_y()..data_window.max_y() {
        let mut row_samples = Vec::with_capacity(
            usize::try_from(samples_per_row).expect("validated row sample count fits usize"),
        );
        for x in data_window.min_x()..data_window.max_x() {
            for channel in 0..names.len() {
                let sample = region
                    .sample_bytes(ChannelIndex::new(channel), x, y)
                    .ok_or_else(|| {
                        internal("encode_dpx", "validated access is missing a channel sample")
                    })?;
                let value = if options.dpx_bit_depth == 8 {
                    u16::from(sample[0])
                } else {
                    u16::from_ne_bytes([sample[0], sample[1]])
                };
                if u32::from(value) > maximum {
                    return Err(invalid(
                        "encode_dpx",
                        "DPX sample exceeds the selected integer bit depth",
                    ));
                }
                row_samples.push(value);
            }
        }
        let row = encode_dpx_row(
            &row_samples,
            options.dpx_bit_depth,
            options.dpx_packing,
            options.dpx_endianness,
        )?;
        debug_assert_eq!(row.len() as u64, row_bytes);
        writer
            .write_all(&row)
            .map_err(|source| dpx_io_error("write_dpx_pixels", source))?;
    }
    Ok(())
}

fn dpx_row_bytes(samples: u64, bit_depth: u8) -> Result<u64> {
    match bit_depth {
        8 => Ok(samples),
        10 => samples
            .checked_add(2)
            .and_then(|value| value.checked_div(3))
            .and_then(|words| words.checked_mul(4))
            .ok_or_else(|| exhausted("dpx_row_bytes", "DPX 10-bit row size overflows")),
        12 | 16 => samples
            .checked_mul(2)
            .ok_or_else(|| exhausted("dpx_row_bytes", "DPX row size overflows")),
        _ => Err(unsupported("dpx_row_bytes", "DPX bit depth is unsupported")),
    }
}

fn decode_dpx_row(
    bytes: &[u8],
    sample_count: usize,
    bit_depth: u8,
    packing: DpxPacking,
    endianness: DpxEndianness,
) -> Result<Vec<u16>> {
    let mut samples = Vec::with_capacity(sample_count);
    match bit_depth {
        8 => samples.extend(
            bytes
                .iter()
                .take(sample_count)
                .map(|value| u16::from(*value)),
        ),
        10 => {
            for word in bytes.chunks_exact(4) {
                let word = dpx_word(word, endianness)?;
                let values = match packing {
                    DpxPacking::FilledMethodA => [
                        ((word >> 22) & 0x3ff) as u16,
                        ((word >> 12) & 0x3ff) as u16,
                        ((word >> 2) & 0x3ff) as u16,
                    ],
                    DpxPacking::FilledMethodB => [
                        ((word >> 20) & 0x3ff) as u16,
                        ((word >> 10) & 0x3ff) as u16,
                        (word & 0x3ff) as u16,
                    ],
                };
                samples.extend(values);
            }
            samples.truncate(sample_count);
        }
        12 => {
            for word in bytes.chunks_exact(2).take(sample_count) {
                let value = match endianness {
                    DpxEndianness::Big => u16::from_be_bytes([word[0], word[1]]),
                    DpxEndianness::Little => u16::from_le_bytes([word[0], word[1]]),
                };
                samples.push(match packing {
                    DpxPacking::FilledMethodA => value >> 4,
                    DpxPacking::FilledMethodB => value & 0x0fff,
                });
            }
        }
        16 => {
            for word in bytes.chunks_exact(2).take(sample_count) {
                samples.push(match endianness {
                    DpxEndianness::Big => u16::from_be_bytes([word[0], word[1]]),
                    DpxEndianness::Little => u16::from_le_bytes([word[0], word[1]]),
                });
            }
        }
        _ => return Err(unsupported("decode_dpx", "DPX bit depth is unsupported")),
    }
    if samples.len() != sample_count {
        return Err(corrupt(
            "decode_dpx",
            "DPX row contains fewer samples than declared",
        ));
    }
    Ok(samples)
}

fn encode_dpx_row(
    samples: &[u16],
    bit_depth: u8,
    packing: DpxPacking,
    endianness: DpxEndianness,
) -> Result<Vec<u8>> {
    let capacity = usize::try_from(dpx_row_bytes(samples.len() as u64, bit_depth)?)
        .map_err(|_| exhausted("encode_dpx", "DPX row exceeds the host address space"))?;
    let mut bytes = Vec::with_capacity(capacity);
    match bit_depth {
        8 => bytes.extend(samples.iter().map(|value| *value as u8)),
        10 => {
            for group in samples.chunks(3) {
                let a = u32::from(group[0]);
                let b = u32::from(group.get(1).copied().unwrap_or(0));
                let c = u32::from(group.get(2).copied().unwrap_or(0));
                let word = match packing {
                    DpxPacking::FilledMethodA => (a << 22) | (b << 12) | (c << 2),
                    DpxPacking::FilledMethodB => (a << 20) | (b << 10) | c,
                };
                let encoded = match endianness {
                    DpxEndianness::Big => word.to_be_bytes(),
                    DpxEndianness::Little => word.to_le_bytes(),
                };
                bytes.extend_from_slice(&encoded);
            }
        }
        12 => {
            for sample in samples {
                let word = match packing {
                    DpxPacking::FilledMethodA => sample << 4,
                    DpxPacking::FilledMethodB => *sample,
                };
                let encoded = match endianness {
                    DpxEndianness::Big => word.to_be_bytes(),
                    DpxEndianness::Little => word.to_le_bytes(),
                };
                bytes.extend_from_slice(&encoded);
            }
        }
        16 => {
            for sample in samples {
                let encoded = match endianness {
                    DpxEndianness::Big => sample.to_be_bytes(),
                    DpxEndianness::Little => sample.to_le_bytes(),
                };
                bytes.extend_from_slice(&encoded);
            }
        }
        _ => return Err(unsupported("encode_dpx", "DPX bit depth is unsupported")),
    }
    Ok(bytes)
}

fn dpx_u16(bytes: &[u8], offset: usize, endianness: DpxEndianness) -> Result<u16> {
    let bytes = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| corrupt("decode_dpx", "DPX header field lies outside the file"))?;
    Ok(match endianness {
        DpxEndianness::Big => u16::from_be_bytes([bytes[0], bytes[1]]),
        DpxEndianness::Little => u16::from_le_bytes([bytes[0], bytes[1]]),
    })
}

fn dpx_u32(bytes: &[u8], offset: usize, endianness: DpxEndianness) -> Result<u32> {
    let bytes = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| corrupt("decode_dpx", "DPX header field lies outside the file"))?;
    Ok(match endianness {
        DpxEndianness::Big => u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        DpxEndianness::Little => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
    })
}

fn dpx_word(bytes: &[u8], endianness: DpxEndianness) -> Result<u32> {
    if bytes.len() != 4 {
        return Err(corrupt("decode_dpx", "DPX packed word is truncated"));
    }
    Ok(match endianness {
        DpxEndianness::Big => u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        DpxEndianness::Little => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
    })
}

fn put_dpx_u16(
    bytes: &mut [u8],
    offset: usize,
    value: u16,
    endianness: DpxEndianness,
) -> Result<()> {
    let destination = bytes
        .get_mut(offset..offset + 2)
        .ok_or_else(|| internal("encode_dpx", "DPX header field is out of bounds"))?;
    let encoded = match endianness {
        DpxEndianness::Big => value.to_be_bytes(),
        DpxEndianness::Little => value.to_le_bytes(),
    };
    destination.copy_from_slice(&encoded);
    Ok(())
}

fn put_dpx_u32(
    bytes: &mut [u8],
    offset: usize,
    value: u32,
    endianness: DpxEndianness,
) -> Result<()> {
    let destination = bytes
        .get_mut(offset..offset + 4)
        .ok_or_else(|| internal("encode_dpx", "DPX header field is out of bounds"))?;
    let encoded = match endianness {
        DpxEndianness::Big => value.to_be_bytes(),
        DpxEndianness::Little => value.to_le_bytes(),
    };
    destination.copy_from_slice(&encoded);
    Ok(())
}

fn dpx_orientation_code(orientation: Option<ImageOrientation>) -> Result<u16> {
    match orientation.unwrap_or(ImageOrientation::TopLeft) {
        ImageOrientation::TopLeft => Ok(0),
        ImageOrientation::TopRight => Ok(1),
        ImageOrientation::BottomLeft => Ok(2),
        ImageOrientation::BottomRight => Ok(3),
        _ => Err(unsupported(
            "encode_dpx",
            "axis-swapped orientation is not supported by the current DPX adapter",
        )),
    }
}

fn dpx_metadata_u8(metadata: &ImageMetadata, key: &str, default: u8) -> Result<u8> {
    match metadata.get(key) {
        Some(ImageMetadataValue::Unsigned(value)) => u8::try_from(*value)
            .map_err(|_| invalid("encode_dpx", "DPX metadata value must fit u8")),
        Some(_) => Err(invalid(
            "encode_dpx",
            "DPX metadata value has the wrong type",
        )),
        None => Ok(default),
    }
}

fn dpx_metadata_u32(metadata: &ImageMetadata, key: &str) -> Result<Option<u32>> {
    match metadata.get(key) {
        Some(ImageMetadataValue::Unsigned(value)) => u32::try_from(*value)
            .map(Some)
            .map_err(|_| invalid("encode_dpx", "DPX metadata value must fit u32")),
        Some(_) => Err(invalid(
            "encode_dpx",
            "DPX metadata value has the wrong type",
        )),
        None => Ok(None),
    }
}

fn dpx_io_error(operation: &'static str, source: std::io::Error) -> Error {
    let category = if source.kind() == std::io::ErrorKind::UnexpectedEof {
        ErrorCategory::CorruptData
    } else {
        ErrorCategory::Unavailable
    };
    Error::with_source(
        category,
        Recoverability::UserCorrectable,
        "DPX byte stream operation failed",
        source,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn checked_product(values: &[u64], operation: &'static str) -> Result<usize> {
    let value = values.iter().try_fold(1_u64, |product, value| {
        product.checked_mul(*value).ok_or_else(|| {
            exhausted(
                operation,
                "image byte count exceeds the supported address space",
            )
        })
    })?;
    usize::try_from(value)
        .map_err(|_| exhausted(operation, "image byte count exceeds the host address space"))
}

fn image_read_error(format: StillImageFormat, source: image::ImageError) -> Error {
    let category = match source {
        image::ImageError::Limits(_) => ErrorCategory::ResourceExhausted,
        image::ImageError::Unsupported(_) => ErrorCategory::Unsupported,
        _ => ErrorCategory::CorruptData,
    };
    Error::with_source(
        category,
        Recoverability::UserCorrectable,
        "still-image decoder rejected the input",
        source,
    )
    .with_context(format_context("decode_raster", format))
}

fn image_write_error(format: StillImageFormat, source: image::ImageError) -> Error {
    let category = match source {
        image::ImageError::Limits(_) => ErrorCategory::ResourceExhausted,
        image::ImageError::Unsupported(_) => ErrorCategory::Unsupported,
        _ => ErrorCategory::InvalidInput,
    };
    Error::with_source(
        category,
        Recoverability::UserCorrectable,
        "still-image encoder rejected the image",
        source,
    )
    .with_context(format_context("encode_raster", format))
}

fn file_error(operation: &'static str, source: std::io::Error) -> Error {
    let category = match source.kind() {
        std::io::ErrorKind::NotFound => ErrorCategory::NotFound,
        std::io::ErrorKind::PermissionDenied => ErrorCategory::PermissionDenied,
        _ => ErrorCategory::Unavailable,
    };
    Error::with_source(
        category,
        Recoverability::UserCorrectable,
        "still-image file operation failed",
        source,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn format_context(operation: &'static str, format: StillImageFormat) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation).with_field("format", format.code())
}

fn invalid(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unsupported(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn exhausted(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn corrupt(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn internal(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
