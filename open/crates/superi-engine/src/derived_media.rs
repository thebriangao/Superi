//! Proxy and optimized-media generation through codec-neutral engine orchestration.

use std::sync::Arc;

use sha2::{Digest, Sha256};
use superi_cache::key::RenderSettingsFingerprint;
use superi_cache::proxy::{
    DerivedMediaArtifact, DerivedMediaCatalog, DerivedMediaPurpose, DerivedMediaQuality,
    DerivedMediaRequest, GeneratedMedia,
};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::ChannelPosition;
use superi_media_io::backend::{BackendRegistry, BackendRequirement, FallbackPolicy};
use superi_media_io::demux::{MetadataValue, Packet};
use superi_media_io::encode::{
    EncodeInput, EncodeOutput, Encoder, EncoderConfig, EncoderMediaFormat,
};
use superi_media_io::operation::OperationContext;

const RENDER_SETTINGS_SCHEMA: &str = "superi.engine.derived-media-render-settings.v1";
const ENCODED_MEDIA_CONTENT_DOMAIN: &[u8] = b"superi.engine.encoded-derived-media.v1\0";

/// Complete elementary packets produced for one proxy or optimized-media request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedDerivedMedia {
    config: EncoderConfig,
    packets: Arc<[Packet]>,
    encoded_byte_len: u64,
}

impl EncodedDerivedMedia {
    /// Returns the exact encoder configuration used for generation.
    #[must_use]
    pub const fn config(&self) -> &EncoderConfig {
        &self.config
    }

    /// Returns complete encoded packets in encoder output order.
    #[must_use]
    pub fn packets(&self) -> &[Packet] {
        &self.packets
    }

    /// Returns the sum of compressed packet payload bytes.
    #[must_use]
    pub const fn encoded_byte_len(&self) -> u64 {
        self.encoded_byte_len
    }
}

/// Derives the exact render-settings identity required by one generation request.
///
/// The fingerprint covers purpose, quality, codec, stream, timebase, decoded representation, color
/// meaning, alpha meaning, and channel order. The caller must produce input frames or audio blocks
/// matching the same encoder configuration.
pub fn derived_media_render_settings(
    config: &EncoderConfig,
    purpose: DerivedMediaPurpose,
    quality: DerivedMediaQuality,
) -> Result<RenderSettingsFingerprint> {
    let mut bytes = Vec::new();
    append_text(&mut bytes, RENDER_SETTINGS_SCHEMA);
    append_text(&mut bytes, purpose.code());
    append_text(&mut bytes, quality.code());
    bytes.extend_from_slice(&config.stream_id().value().to_be_bytes());
    append_text(&mut bytes, config.codec().as_str());
    bytes.extend_from_slice(&config.timebase().numerator().to_be_bytes());
    bytes.extend_from_slice(&config.timebase().denominator().to_be_bytes());

    match config.media_format() {
        EncoderMediaFormat::Video(format) => {
            append_text(&mut bytes, "video");
            bytes.extend_from_slice(&format.width().to_be_bytes());
            bytes.extend_from_slice(&format.height().to_be_bytes());
            append_text(&mut bytes, format.pixel_format().code());
            append_text(&mut bytes, format.color_space().primaries().code());
            append_text(&mut bytes, format.color_space().transfer().code());
            append_text(&mut bytes, format.color_space().matrix().code());
            append_text(&mut bytes, format.color_space().range().code());
            append_text(&mut bytes, format.alpha_mode().code());
        }
        EncoderMediaFormat::Audio(format) => {
            append_text(&mut bytes, "audio");
            bytes.extend_from_slice(&format.sample_rate().to_be_bytes());
            append_text(&mut bytes, format.sample_format().code());
            append_count(&mut bytes, format.channel_layout().len())?;
            for position in format.channel_layout().positions() {
                append_channel_position(&mut bytes, *position)?;
            }
        }
        _ => {
            return Err(unsupported(
                "derive_media_render_settings",
                "encoder media format has no canonical derived-media encoding",
            ));
        }
    }

    Ok(RenderSettingsFingerprint::from_canonical_bytes(bytes))
}

/// Generates or reuses one exact complete encoded derived-media artifact.
///
/// Backend fallback is disallowed. Packets remain private until the encoder reaches end of stream
/// and the operation passes its final cancellation check. A failure therefore cannot publish
/// partial media or replace a prior exact artifact.
pub fn generate_derived_media<I>(
    catalog: &mut DerivedMediaCatalog<EncodedDerivedMedia>,
    request: DerivedMediaRequest,
    registry: &BackendRegistry,
    config: EncoderConfig,
    inputs: I,
    operation: &OperationContext,
) -> Result<Arc<DerivedMediaArtifact<EncodedDerivedMedia>>>
where
    I: IntoIterator<Item = EncodeInput>,
{
    operation.check("generate_derived_media")?;
    let expected = derived_media_render_settings(&config, request.purpose(), request.quality())?;
    if expected != request.render_settings() {
        return Err(conflict(
            "generate_derived_media",
            "generation request render settings do not match the encoder configuration",
        ));
    }

    catalog.get_or_generate(request, || {
        operation.check("start_derived_media_generation")?;
        let requirement = BackendRequirement::encode(config.codec().clone());
        let selection = registry.select(&requirement, FallbackPolicy::Disallow)?;
        let mut encoder = selection.primary().create_encoder(&config, operation)?;
        encode_complete_media(&mut *encoder, config, inputs, operation)
    })
}

fn encode_complete_media<I>(
    encoder: &mut dyn Encoder,
    config: EncoderConfig,
    inputs: I,
    operation: &OperationContext,
) -> Result<GeneratedMedia<EncodedDerivedMedia>>
where
    I: IntoIterator<Item = EncodeInput>,
{
    let mut packets = Vec::new();
    let mut input_count = 0_u64;
    for input in inputs {
        operation.check("send_derived_media_input")?;
        encoder.send(input, operation)?;
        input_count = input_count.checked_add(1).ok_or_else(|| {
            resource_exhausted(
                "send_derived_media_input",
                "derived-media input count exceeds the supported range",
            )
        })?;
        drain_before_flush(encoder, &mut packets, operation)?;
    }
    if input_count == 0 {
        return Err(invalid(
            "generate_derived_media",
            "derived-media generation requires at least one input",
        ));
    }

    operation.check("flush_derived_media_encoder")?;
    encoder.flush(operation)?;
    drain_after_flush(encoder, &mut packets, operation)?;
    operation.check("publish_derived_media")?;

    validate_packets(&config, &packets)?;
    let (content_fingerprint, encoded_byte_len) = fingerprint_packets(&packets)?;
    let payload = EncodedDerivedMedia {
        config,
        packets: Arc::from(packets),
        encoded_byte_len,
    };
    GeneratedMedia::new(payload, content_fingerprint, encoded_byte_len)
}

fn validate_packets(config: &EncoderConfig, packets: &[Packet]) -> Result<()> {
    for packet in packets {
        if packet.stream_id() != config.stream_id() {
            return Err(internal(
                "validate_derived_media_packets",
                "encoder output stream does not match its derived-media configuration",
            ));
        }
        if packet.timing().timebase() != config.timebase() {
            return Err(internal(
                "validate_derived_media_packets",
                "encoder output timebase does not match its derived-media configuration",
            ));
        }
        if packet.data().is_empty() {
            return Err(internal(
                "validate_derived_media_packets",
                "encoder produced an empty derived-media packet",
            ));
        }
    }
    Ok(())
}

fn drain_before_flush(
    encoder: &mut dyn Encoder,
    packets: &mut Vec<Packet>,
    operation: &OperationContext,
) -> Result<()> {
    loop {
        operation.check("drain_derived_media_encoder")?;
        match encoder.receive(operation)? {
            EncodeOutput::Packet(packet) => packets.push(packet),
            EncodeOutput::NeedInput => return Ok(()),
            EncodeOutput::EndOfStream => {
                return Err(internal(
                    "drain_derived_media_encoder",
                    "encoder ended before derived-media input was flushed",
                ));
            }
            _ => {
                return Err(unsupported(
                    "drain_derived_media_encoder",
                    "encoder returned an unknown output lifecycle state",
                ));
            }
        }
    }
}

fn drain_after_flush(
    encoder: &mut dyn Encoder,
    packets: &mut Vec<Packet>,
    operation: &OperationContext,
) -> Result<()> {
    loop {
        operation.check("finish_derived_media_encoder")?;
        match encoder.receive(operation)? {
            EncodeOutput::Packet(packet) => packets.push(packet),
            EncodeOutput::EndOfStream => return Ok(()),
            EncodeOutput::NeedInput => {
                return Err(internal(
                    "finish_derived_media_encoder",
                    "encoder requested input after derived-media flush",
                ));
            }
            _ => {
                return Err(unsupported(
                    "finish_derived_media_encoder",
                    "encoder returned an unknown output lifecycle state",
                ));
            }
        }
    }
}

fn fingerprint_packets(packets: &[Packet]) -> Result<([u8; 32], u64)> {
    if packets.is_empty() {
        return Err(internal(
            "fingerprint_derived_media",
            "encoder reached end of stream without producing derived-media packets",
        ));
    }
    let mut hasher = Sha256::new();
    hasher.update(ENCODED_MEDIA_CONTENT_DOMAIN);
    update_count(&mut hasher, packets.len())?;
    let mut encoded_byte_len = 0_u64;
    for packet in packets {
        hasher.update(packet.stream_id().value().to_be_bytes());
        update_bytes(&mut hasher, packet.data())?;
        encoded_byte_len = encoded_byte_len
            .checked_add(u64::try_from(packet.data().len()).map_err(|_| {
                resource_exhausted(
                    "fingerprint_derived_media",
                    "packet length exceeds the supported range",
                )
            })?)
            .ok_or_else(|| {
                resource_exhausted(
                    "fingerprint_derived_media",
                    "encoded derived-media length exceeds the supported range",
                )
            })?;

        let timing = packet.timing();
        hasher.update(timing.timebase().numerator().to_be_bytes());
        hasher.update(timing.timebase().denominator().to_be_bytes());
        update_optional_i64(
            &mut hasher,
            timing.presentation_time().map(|time| time.value()),
        );
        update_optional_i64(&mut hasher, timing.decode_time().map(|time| time.value()));
        update_optional_u64(
            &mut hasher,
            timing.duration().map(|duration| duration.value()),
        );
        hasher.update([u8::from(packet.is_keyframe())]);

        let metadata = packet.metadata().iter().collect::<Vec<_>>();
        update_count(&mut hasher, metadata.len())?;
        for (key, value) in metadata {
            update_bytes(&mut hasher, key.as_bytes())?;
            update_metadata(&mut hasher, value)?;
        }
    }
    if encoded_byte_len == 0 {
        return Err(internal(
            "fingerprint_derived_media",
            "encoder produced only empty derived-media packets",
        ));
    }
    Ok((hasher.finalize().into(), encoded_byte_len))
}

fn update_metadata(hasher: &mut Sha256, value: &MetadataValue) -> Result<()> {
    match value {
        MetadataValue::Text(value) => {
            hasher.update([0]);
            update_bytes(hasher, value.as_bytes())?;
        }
        MetadataValue::Signed(value) => {
            hasher.update([1]);
            hasher.update(value.to_be_bytes());
        }
        MetadataValue::Unsigned(value) => {
            hasher.update([2]);
            hasher.update(value.to_be_bytes());
        }
        MetadataValue::Boolean(value) => hasher.update([3, u8::from(*value)]),
        MetadataValue::Bytes(value) => {
            hasher.update([4]);
            update_bytes(hasher, value)?;
        }
        _ => {
            return Err(unsupported(
                "fingerprint_derived_media",
                "packet metadata has no canonical derived-media encoding",
            ));
        }
    }
    Ok(())
}

fn append_channel_position(bytes: &mut Vec<u8>, position: ChannelPosition) -> Result<()> {
    let code = match position {
        ChannelPosition::FrontLeft => "front_left",
        ChannelPosition::FrontRight => "front_right",
        ChannelPosition::FrontCenter => "front_center",
        ChannelPosition::LowFrequency => "low_frequency",
        ChannelPosition::BackLeft => "back_left",
        ChannelPosition::BackRight => "back_right",
        ChannelPosition::FrontLeftOfCenter => "front_left_of_center",
        ChannelPosition::FrontRightOfCenter => "front_right_of_center",
        ChannelPosition::BackCenter => "back_center",
        ChannelPosition::SideLeft => "side_left",
        ChannelPosition::SideRight => "side_right",
        ChannelPosition::TopCenter => "top_center",
        ChannelPosition::TopFrontLeft => "top_front_left",
        ChannelPosition::TopFrontCenter => "top_front_center",
        ChannelPosition::TopFrontRight => "top_front_right",
        ChannelPosition::TopBackLeft => "top_back_left",
        ChannelPosition::TopBackCenter => "top_back_center",
        ChannelPosition::TopBackRight => "top_back_right",
        ChannelPosition::Discrete(index) => {
            append_text(bytes, "discrete");
            bytes.extend_from_slice(&index.to_be_bytes());
            return Ok(());
        }
        _ => {
            return Err(unsupported(
                "derive_media_render_settings",
                "audio channel position has no canonical derived-media encoding",
            ));
        }
    };
    append_text(bytes, code);
    Ok(())
}

fn append_text(bytes: &mut Vec<u8>, value: &str) {
    let length = u64::try_from(value.len()).expect("static and codec text lengths fit u64");
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

fn append_count(bytes: &mut Vec<u8>, count: usize) -> Result<()> {
    let count = u64::try_from(count).map_err(|_| {
        resource_exhausted(
            "derive_media_render_settings",
            "render-setting collection exceeds the supported range",
        )
    })?;
    bytes.extend_from_slice(&count.to_be_bytes());
    Ok(())
}

fn update_bytes(hasher: &mut Sha256, bytes: &[u8]) -> Result<()> {
    update_count(hasher, bytes.len())?;
    hasher.update(bytes);
    Ok(())
}

fn update_count(hasher: &mut Sha256, count: usize) -> Result<()> {
    let count = u64::try_from(count).map_err(|_| {
        resource_exhausted(
            "fingerprint_derived_media",
            "derived-media collection exceeds the supported range",
        )
    })?;
    hasher.update(count.to_be_bytes());
    Ok(())
}

fn update_optional_i64(hasher: &mut Sha256, value: Option<i64>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hasher.update(value.to_be_bytes());
        }
        None => hasher.update([0]),
    }
}

fn update_optional_u64(hasher: &mut Sha256, value: Option<u64>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hasher.update(value.to_be_bytes());
        }
        None => hasher.update([0]),
    }
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-engine.derived-media", operation))
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-engine.derived-media", operation))
}

fn resource_exhausted(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::Retryable,
        message,
    )
    .with_context(ErrorContext::new("superi-engine.derived-media", operation))
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new("superi-engine.derived-media", operation))
}

fn internal(operation: &'static str, message: &'static str) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
        .with_context(ErrorContext::new("superi-engine.derived-media", operation))
}
