//! Bounded, replaceable media preview generation for the desktop inspector.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use base64::Engine as _;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder, ImageFormat, ImageReader, Limits};
use serde::{Deserialize, Serialize};
use superi_codecs_rs::pcm::PcmBackend;
use superi_core::color_space::ColorSpace;
use superi_core::geometry::PixelBounds;
use superi_core::ids::MediaId;
use superi_core::pixel::{AlphaMode, ChannelPosition, PixelFormat, SampleFormat};
use superi_image::preview::{generate_thumbnail, ThumbnailRequest, WaveformRasterStyle};
use superi_image::value::{Image as SuperiImage, ImageDescriptor, ImageSamples};
use superi_media_io::audio_io::{AudioBlock, AudioFormat};
use superi_media_io::backend::MediaBackend;
use superi_media_io::decode::{DecodeOutput, Decoder, DecoderConfig};
use superi_media_io::demux::{MediaSource, SourceLocation, SourceRequest};
use superi_media_io::operation::{MediaPriority, OperationContext};
use superi_media_io::pcm::{PcmContainerSource, PcmEncoding};
use superi_media_io::preview::{generate_audio_waveform_image, WaveformRequest};
use superi_media_io::read::ReadOutcome;

use super::{DesktopImportedMediaKind, MediaBrowserItem};

const THUMBNAIL_WIDTH: u32 = 320;
const THUMBNAIL_HEIGHT: u32 = 180;
const FILMSTRIP_WIDTH: u32 = 192;
const FILMSTRIP_HEIGHT: u32 = 108;
const PREVIEW_WIDTH: u32 = 960;
const PREVIEW_HEIGHT: u32 = 540;
const WAVEFORM_WIDTH: u32 = 1_024;
const MAX_FILMSTRIP_FRAMES: usize = 6;
const MAX_IMAGE_SOURCE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_IMAGE_DIMENSION: u32 = 32_768;
const MAX_DECODED_IMAGE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_DECODED_AUDIO_BYTES: u64 = 128 * 1024 * 1024;
const MAX_CHANNELS: usize = 32;
const MAX_PNG_BYTES: usize = 4 * 1024 * 1024;

const IMAGE_UNAVAILABLE: &str = "Image preview generation is unavailable for this source.";
const AUDIO_UNAVAILABLE: &str = "Audio waveform generation is unavailable for this source.";
const IMAGE_WAVEFORM_UNAVAILABLE: &str = "Still images do not contain an audio waveform.";
const AUDIO_FRAME_UNAVAILABLE: &str = "Audio sources do not contain visual source frames.";
const FORMAT_UNAVAILABLE: &str = "No bounded preview decoder is available for this media format.";

/// Exact revision and source identity required for one replaceable preview request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaPreviewRequest {
    pub expected_project_revision: u64,
    pub expected_library_revision: u64,
    pub media_id: String,
    pub expected_freshness: String,
}

/// One generated PNG surface with its source position retained explicitly.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewImageArtifact {
    data_url: String,
    width: u32,
    height: u32,
    source_index: Option<u64>,
    source_count: u64,
}

/// Ordered representative source frames for one still or image sequence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilmstripArtifact {
    frames: Vec<PreviewImageArtifact>,
    source_count: u64,
}

/// A channel-separated waveform paired with its exact decoded sample clock.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaveformArtifact {
    image: PreviewImageArtifact,
    start_sample: i64,
    sample_rate: u32,
    frame_count: u64,
    channel_layout: Vec<String>,
}

/// One independently available generated preview product.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum MediaPreviewProduct<T> {
    Ready { artifact: T },
    Unavailable { reason: String },
}

impl<T> MediaPreviewProduct<T> {
    fn ready(artifact: T) -> Self {
        Self::Ready { artifact }
    }

    fn unavailable(reason: &'static str) -> Self {
        Self::Unavailable {
            reason: reason.to_owned(),
        }
    }
}

/// Complete replaceable preview state for one exact imported-media fingerprint.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaPreviewBundle {
    media_id: String,
    freshness: String,
    thumbnail: MediaPreviewProduct<PreviewImageArtifact>,
    filmstrip: MediaPreviewProduct<FilmstripArtifact>,
    waveform: MediaPreviewProduct<WaveformArtifact>,
    preview: MediaPreviewProduct<PreviewImageArtifact>,
}

pub(super) fn generate(item: &MediaBrowserItem) -> MediaPreviewBundle {
    let extension = item
        .source_paths
        .first()
        .and_then(|path| Path::new(path).extension())
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if item.kind == DesktopImportedMediaKind::ImageSequence || is_supported_image(&extension) {
        return generate_image_bundle(item);
    }
    if extension == "wav" {
        return generate_audio_bundle(item);
    }
    unavailable_bundle(item, FORMAT_UNAVAILABLE)
}

fn unavailable_bundle(item: &MediaBrowserItem, reason: &'static str) -> MediaPreviewBundle {
    MediaPreviewBundle {
        media_id: item.media_id.clone(),
        freshness: item.content_fingerprint.clone(),
        thumbnail: MediaPreviewProduct::unavailable(reason),
        filmstrip: MediaPreviewProduct::unavailable(reason),
        waveform: MediaPreviewProduct::unavailable(reason),
        preview: MediaPreviewProduct::unavailable(reason),
    }
}

fn generate_image_bundle(item: &MediaBrowserItem) -> MediaPreviewBundle {
    let source_count = item.source_paths.len();
    let selected = representative_indices(source_count, MAX_FILMSTRIP_FRAMES);
    let preview_index = source_count.checked_sub(1).map(|last| last / 2);
    let mut required = selected.clone();
    if source_count > 0 {
        required.push(0);
    }
    if let Some(preview_index) = preview_index {
        required.push(preview_index);
    }
    required.sort_unstable();
    required.dedup();

    let decoded = required
        .into_iter()
        .map(|index| decode_image(Path::new(&item.source_paths[index])).map(|image| (index, image)))
        .collect::<Result<BTreeMap<_, _>, _>>();
    let Ok(decoded) = decoded else {
        return MediaPreviewBundle {
            media_id: item.media_id.clone(),
            freshness: item.content_fingerprint.clone(),
            thumbnail: MediaPreviewProduct::unavailable(IMAGE_UNAVAILABLE),
            filmstrip: MediaPreviewProduct::unavailable(IMAGE_UNAVAILABLE),
            waveform: MediaPreviewProduct::unavailable(IMAGE_WAVEFORM_UNAVAILABLE),
            preview: MediaPreviewProduct::unavailable(IMAGE_UNAVAILABLE),
        };
    };
    let source_count_u64 = u64::try_from(source_count).unwrap_or(u64::MAX);

    let thumbnail = decoded
        .get(&0)
        .ok_or(())
        .and_then(|source| {
            scaled_artifact(
                source,
                THUMBNAIL_WIDTH,
                THUMBNAIL_HEIGHT,
                Some(0),
                source_count_u64,
            )
        })
        .map_or_else(
            |_| MediaPreviewProduct::unavailable(IMAGE_UNAVAILABLE),
            MediaPreviewProduct::ready,
        );

    let preview = preview_index
        .and_then(|index| decoded.get(&index).map(|source| (index, source)))
        .ok_or(())
        .and_then(|(index, source)| {
            scaled_artifact(
                source,
                PREVIEW_WIDTH,
                PREVIEW_HEIGHT,
                u64::try_from(index).ok(),
                source_count_u64,
            )
        })
        .map_or_else(
            |_| MediaPreviewProduct::unavailable(IMAGE_UNAVAILABLE),
            MediaPreviewProduct::ready,
        );

    let filmstrip = selected
        .into_iter()
        .map(|index| {
            decoded.get(&index).ok_or(()).and_then(|source| {
                scaled_artifact(
                    source,
                    FILMSTRIP_WIDTH,
                    FILMSTRIP_HEIGHT,
                    u64::try_from(index).ok(),
                    source_count_u64,
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .and_then(|frames| {
            if frames.is_empty() {
                Err(())
            } else {
                Ok(FilmstripArtifact {
                    frames,
                    source_count: source_count_u64,
                })
            }
        })
        .map_or_else(
            |_| MediaPreviewProduct::unavailable(IMAGE_UNAVAILABLE),
            MediaPreviewProduct::ready,
        );

    MediaPreviewBundle {
        media_id: item.media_id.clone(),
        freshness: item.content_fingerprint.clone(),
        thumbnail,
        filmstrip,
        waveform: MediaPreviewProduct::unavailable(IMAGE_WAVEFORM_UNAVAILABLE),
        preview,
    }
}

fn generate_audio_bundle(item: &MediaBrowserItem) -> MediaPreviewBundle {
    let generated = item
        .source_paths
        .first()
        .ok_or(())
        .and_then(|path| decode_waveform(item, Path::new(path)));
    let (waveform, preview) = generated.map_or_else(
        |_| {
            (
                MediaPreviewProduct::unavailable(AUDIO_UNAVAILABLE),
                MediaPreviewProduct::unavailable(AUDIO_UNAVAILABLE),
            )
        },
        |artifact| {
            let preview = artifact.image.clone();
            (
                MediaPreviewProduct::ready(artifact),
                MediaPreviewProduct::ready(preview),
            )
        },
    );
    MediaPreviewBundle {
        media_id: item.media_id.clone(),
        freshness: item.content_fingerprint.clone(),
        thumbnail: MediaPreviewProduct::unavailable(AUDIO_FRAME_UNAVAILABLE),
        filmstrip: MediaPreviewProduct::unavailable(AUDIO_FRAME_UNAVAILABLE),
        waveform,
        preview,
    }
}

fn is_supported_image(extension: &str) -> bool {
    matches!(extension, "png" | "jpg" | "jpeg" | "tif" | "tiff")
}

fn representative_indices(source_count: usize, maximum: usize) -> Vec<usize> {
    if source_count == 0 || maximum == 0 {
        return Vec::new();
    }
    if source_count <= maximum {
        return (0..source_count).collect();
    }
    if maximum == 1 {
        return vec![source_count / 2];
    }
    let last = source_count - 1;
    (0..maximum)
        .map(|slot| slot * last / (maximum - 1))
        .collect()
}

fn decode_image(path: &Path) -> Result<SuperiImage, ()> {
    let metadata = std::fs::metadata(path).map_err(|_| ())?;
    if !metadata.is_file() || metadata.len() > MAX_IMAGE_SOURCE_BYTES {
        return Err(());
    }
    let mut reader = ImageReader::open(path)
        .map_err(|_| ())?
        .with_guessed_format()
        .map_err(|_| ())?;
    if !matches!(
        reader.format(),
        Some(
            ImageFormat::Png
                | ImageFormat::Jpeg
                | ImageFormat::Tiff
                | ImageFormat::WebP
                | ImageFormat::Bmp
                | ImageFormat::Tga
        )
    ) {
        return Err(());
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODED_IMAGE_BYTES);
    reader.limits(limits);
    let rgba = reader.decode().map_err(|_| ())?.into_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    let byte_count = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(())?;
    if byte_count == 0 || byte_count > MAX_DECODED_IMAGE_BYTES {
        return Err(());
    }
    let bounds = PixelBounds::from_origin_size(0, 0, width, height).map_err(|_| ())?;
    let descriptor = ImageDescriptor::new(
        bounds,
        bounds,
        PixelFormat::Rgba8Unorm,
        ColorSpace::SRGB,
        AlphaMode::Straight,
    )
    .map_err(|_| ())?;
    SuperiImage::new(descriptor, ImageSamples::from_u8(rgba.into_raw())).map_err(|_| ())
}

fn scaled_artifact(
    source: &SuperiImage,
    max_width: u32,
    max_height: u32,
    source_index: Option<u64>,
    source_count: u64,
) -> Result<PreviewImageArtifact, ()> {
    let request = ThumbnailRequest::new(max_width, max_height).map_err(|_| ())?;
    let image = generate_thumbnail(source, request).map_err(|_| ())?;
    encode_png_artifact(&image, source_index, source_count)
}

fn encode_png_artifact(
    image: &SuperiImage,
    source_index: Option<u64>,
    source_count: u64,
) -> Result<PreviewImageArtifact, ()> {
    let descriptor = image.descriptor();
    if descriptor.pixel_format() != PixelFormat::Rgba8Unorm
        || descriptor.alpha_mode() != AlphaMode::Straight
        || descriptor.color_space() != ColorSpace::SRGB
    {
        return Err(());
    }
    let bounds = descriptor.data_window();
    let width = bounds.width();
    let height = bounds.height();
    if width == 0 || height == 0 {
        return Err(());
    }
    let samples = image.samples().u8_values().ok_or(())?;
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(samples, width, height, ExtendedColorType::Rgba8)
        .map_err(|_| ())?;
    if png.is_empty() || png.len() > MAX_PNG_BYTES {
        return Err(());
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(png);
    Ok(PreviewImageArtifact {
        data_url: format!("data:image/png;base64,{encoded}"),
        width,
        height,
        source_index,
        source_count,
    })
}

fn decode_waveform(item: &MediaBrowserItem, path: &Path) -> Result<WaveformArtifact, ()> {
    let media_id = item.media_id.parse::<MediaId>().map_err(|_| ())?;
    let operation = OperationContext::new(MediaPriority::Background)
        .with_timeout(Duration::from_secs(30))
        .map_err(|_| ())?;
    let request = SourceRequest::new(media_id, SourceLocation::Path(path.to_owned()));
    let mut source = PcmContainerSource::open(&request, &operation).map_err(|_| ())?;
    let stored = source.format();
    if stored.channel_layout().len() > MAX_CHANNELS {
        return Err(());
    }
    let decoded_bytes = source
        .frame_count()
        .checked_mul(u64::from(stored.block_align()))
        .ok_or(())?;
    if decoded_bytes == 0 || decoded_bytes > MAX_DECODED_AUDIO_BYTES {
        return Err(());
    }
    let sample_format = decoded_sample_format(stored.encoding(), stored.bits_per_sample())?;
    let audio_format = AudioFormat::new(
        stored.sample_rate(),
        sample_format,
        stored.channel_layout().clone(),
    )
    .map_err(|_| ())?;
    let stream = source.info().streams().first().cloned().ok_or(())?;
    let config = DecoderConfig::new(stream)
        .with_audio_format(audio_format)
        .map_err(|_| ())?;
    let backend = PcmBackend::new().map_err(|_| ())?;
    let mut decoder = backend
        .create_decoder(&config, &operation)
        .map_err(|_| ())?;
    let mut blocks = Vec::new();
    loop {
        match source.read_packet(&operation).map_err(|_| ())? {
            ReadOutcome::Complete(packet) => {
                decoder.send_packet(packet, &operation).map_err(|_| ())?;
                if drain_decoder(decoder.as_mut(), &operation, &mut blocks)? {
                    return Err(());
                }
            }
            ReadOutcome::Partial { report, .. } => {
                let _ = report.to_error("generate_media_waveform");
                return Err(());
            }
            ReadOutcome::EndOfStream => break,
            _ => return Err(()),
        }
    }
    decoder.flush(&operation).map_err(|_| ())?;
    if !drain_decoder(decoder.as_mut(), &operation, &mut blocks)? {
        return Err(());
    }
    let decoded_frames = blocks.iter().try_fold(0_u64, |total, block| {
        total.checked_add(block.frame_count()).ok_or(())
    })?;
    if decoded_frames != source.frame_count() {
        return Err(());
    }
    let request =
        WaveformRequest::new(WAVEFORM_WIDTH, WaveformRasterStyle::default()).map_err(|_| ())?;
    let waveform = generate_audio_waveform_image(&blocks, request).map_err(|_| ())?;
    let image = encode_png_artifact(waveform.image(), None, 1)?;
    Ok(WaveformArtifact {
        image,
        start_sample: waveform.start().sample(),
        sample_rate: waveform.start().sample_rate(),
        frame_count: waveform.frame_count(),
        channel_layout: waveform
            .channel_layout()
            .positions()
            .iter()
            .copied()
            .map(channel_position_code)
            .collect(),
    })
}

fn drain_decoder(
    decoder: &mut dyn Decoder,
    operation: &OperationContext,
    blocks: &mut Vec<AudioBlock>,
) -> Result<bool, ()> {
    loop {
        match decoder.receive(operation).map_err(|_| ())? {
            DecodeOutput::Audio(block) => {
                blocks.try_reserve(1).map_err(|_| ())?;
                blocks.push(block);
            }
            DecodeOutput::NeedInput => return Ok(false),
            DecodeOutput::EndOfStream => return Ok(true),
            DecodeOutput::Frame(_) => return Err(()),
            _ => return Err(()),
        }
    }
}

fn decoded_sample_format(encoding: PcmEncoding, bits_per_sample: u16) -> Result<SampleFormat, ()> {
    match (encoding, bits_per_sample) {
        (PcmEncoding::Integer, 8) => Ok(SampleFormat::U8),
        (PcmEncoding::Integer, 16) => Ok(SampleFormat::I16),
        (PcmEncoding::Integer, 24) => Ok(SampleFormat::I24),
        (PcmEncoding::Integer, 32) => Ok(SampleFormat::I32),
        (PcmEncoding::Float, 32) => Ok(SampleFormat::F32),
        (PcmEncoding::Float, 64) => Ok(SampleFormat::F64),
        _ => Err(()),
    }
}

fn channel_position_code(position: ChannelPosition) -> String {
    match position {
        ChannelPosition::FrontLeft => "front_left".to_owned(),
        ChannelPosition::FrontRight => "front_right".to_owned(),
        ChannelPosition::FrontCenter => "front_center".to_owned(),
        ChannelPosition::LowFrequency => "low_frequency".to_owned(),
        ChannelPosition::BackLeft => "back_left".to_owned(),
        ChannelPosition::BackRight => "back_right".to_owned(),
        ChannelPosition::FrontLeftOfCenter => "front_left_of_center".to_owned(),
        ChannelPosition::FrontRightOfCenter => "front_right_of_center".to_owned(),
        ChannelPosition::BackCenter => "back_center".to_owned(),
        ChannelPosition::SideLeft => "side_left".to_owned(),
        ChannelPosition::SideRight => "side_right".to_owned(),
        ChannelPosition::TopCenter => "top_center".to_owned(),
        ChannelPosition::TopFrontLeft => "top_front_left".to_owned(),
        ChannelPosition::TopFrontCenter => "top_front_center".to_owned(),
        ChannelPosition::TopFrontRight => "top_front_right".to_owned(),
        ChannelPosition::TopBackLeft => "top_back_left".to_owned(),
        ChannelPosition::TopBackCenter => "top_back_center".to_owned(),
        ChannelPosition::TopBackRight => "top_back_right".to_owned(),
        ChannelPosition::Discrete(index) => format!("discrete_{index}"),
        _ => "unknown".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::representative_indices;

    #[test]
    fn representative_indices_preserve_endpoints_order_and_bounds() {
        assert_eq!(representative_indices(0, 6), Vec::<usize>::new());
        assert_eq!(representative_indices(3, 6), vec![0, 1, 2]);
        assert_eq!(representative_indices(10, 6), vec![0, 1, 3, 5, 7, 9]);
        assert_eq!(representative_indices(10, 1), vec![5]);
    }
}
