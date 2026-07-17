//! Strict deterministic documents for authored clip-mix state.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::ClipId;
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::serialization::STABLE_PRIMITIVE_SCHEMA_REVISION;

use crate::mixing::{ChannelMap, ClipMixControls, ClipMixState};

const COMPONENT: &str = "superi-audio.serialization";
const CLIP_MIX_FORMAT: &str = "superi.clip-mix";
const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
const MAX_CLIPS: usize = 262_144;
const MAX_ROUTES_PER_CLIP: usize = 1_024;

/// Current incompatible revision of the clip-mix document contract.
pub const CLIP_MIX_FORMAT_REVISION: u32 = 1;

/// Serializes complete authored mix intent into canonical JSON bytes.
pub fn serialize_clip_mix_state(state: &ClipMixState) -> Result<Vec<u8>> {
    if state.iter().len() > MAX_CLIPS {
        return Err(serialization_error(
            ErrorCategory::ResourceExhausted,
            "encode_clip_mix",
            "clip-mix state contains too many clip entries",
        ));
    }
    if state
        .iter()
        .any(|(_, controls)| controls.channel_map().len() > MAX_ROUTES_PER_CLIP)
    {
        return Err(serialization_error(
            ErrorCategory::ResourceExhausted,
            "encode_clip_mix",
            "clip-mix controls contain too many channel routes",
        ));
    }
    let payload = ClipMixPayloadWire::from_state(state);
    let payload_bytes = serde_json::to_vec(&payload).map_err(|source| {
        serialization_error(
            ErrorCategory::InvalidInput,
            "encode_clip_mix",
            "clip-mix payload cannot be represented as JSON",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "encode_clip_mix")
                .with_field("source", source.to_string()),
        )
    })?;
    let envelope = ClipMixEnvelopeRef {
        format: CLIP_MIX_FORMAT,
        format_revision: CLIP_MIX_FORMAT_REVISION,
        primitive_schema_revision: STABLE_PRIMITIVE_SCHEMA_REVISION,
        payload_sha256: sha256_hex(&payload_bytes),
        payload: &payload,
    };
    let document = serde_json::to_vec(&envelope).map_err(|source| {
        serialization_error(
            ErrorCategory::InvalidInput,
            "encode_clip_mix",
            "clip-mix envelope cannot be represented as JSON",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "encode_clip_mix")
                .with_field("source", source.to_string()),
        )
    })?;
    if document.len() > MAX_DOCUMENT_BYTES {
        return Err(serialization_error(
            ErrorCategory::ResourceExhausted,
            "encode_clip_mix",
            "clip-mix document exceeds the supported size limit",
        ));
    }
    Ok(document)
}

/// Decodes, verifies, and reconstructs canonical authored mix intent.
pub fn deserialize_clip_mix_state(document: &[u8]) -> Result<ClipMixState> {
    if document.len() > MAX_DOCUMENT_BYTES {
        return Err(serialization_error(
            ErrorCategory::CorruptData,
            "decode_clip_mix",
            "clip-mix document exceeds the supported size limit",
        ));
    }
    let envelope: ClipMixEnvelopeWire = serde_json::from_slice(document).map_err(|source| {
        serialization_error(
            ErrorCategory::CorruptData,
            "decode_clip_mix",
            "clip-mix document does not match its strict JSON schema",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "decode_clip_mix")
                .with_field("source", source.to_string()),
        )
    })?;
    if envelope.format != CLIP_MIX_FORMAT
        || envelope.format_revision != CLIP_MIX_FORMAT_REVISION
        || envelope.primitive_schema_revision != STABLE_PRIMITIVE_SCHEMA_REVISION
    {
        return Err(serialization_error(
            ErrorCategory::Unsupported,
            "decode_clip_mix",
            "clip-mix document uses an unsupported format revision",
        ));
    }
    if envelope.payload.clips.len() > MAX_CLIPS {
        return Err(serialization_error(
            ErrorCategory::ResourceExhausted,
            "decode_clip_mix",
            "clip-mix document contains too many clip entries",
        ));
    }
    let payload_bytes = serde_json::to_vec(&envelope.payload).map_err(|source| {
        serialization_error(
            ErrorCategory::CorruptData,
            "verify_clip_mix",
            "clip-mix payload cannot be canonicalized for verification",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "verify_clip_mix")
                .with_field("source", source.to_string()),
        )
    })?;
    let expected_digest = sha256_hex(&payload_bytes);
    if envelope.payload_sha256.len() != 64
        || !envelope
            .payload_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || envelope.payload_sha256 != expected_digest
    {
        return Err(serialization_error(
            ErrorCategory::CorruptData,
            "verify_clip_mix",
            "clip-mix payload integrity check failed",
        ));
    }

    let state = envelope.payload.into_state()?;
    let canonical = serialize_clip_mix_state(&state)?;
    if canonical != document {
        return Err(serialization_error(
            ErrorCategory::CorruptData,
            "verify_clip_mix",
            "clip-mix document is not in canonical byte form",
        ));
    }
    Ok(state)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClipMixEnvelopeWire {
    format: String,
    format_revision: u32,
    primitive_schema_revision: u32,
    payload_sha256: String,
    payload: ClipMixPayloadWire,
}

#[derive(Serialize)]
struct ClipMixEnvelopeRef<'a> {
    format: &'static str,
    format_revision: u32,
    primitive_schema_revision: u32,
    payload_sha256: String,
    payload: &'a ClipMixPayloadWire,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClipMixPayloadWire {
    revision: String,
    clips: Vec<ClipMixEntryWire>,
}

impl ClipMixPayloadWire {
    fn from_state(state: &ClipMixState) -> Self {
        Self {
            revision: state.revision().to_string(),
            clips: state
                .iter()
                .map(|(clip_id, controls)| ClipMixEntryWire {
                    clip_id,
                    controls: ClipMixControlsWire::from_controls(controls),
                })
                .collect(),
        }
    }

    fn into_state(self) -> Result<ClipMixState> {
        let revision = parse_canonical_u64(&self.revision, "clip-mix revision")?;
        let controls = self
            .clips
            .into_iter()
            .map(|entry| Ok((entry.clip_id, entry.controls.into_controls()?)))
            .collect::<Result<Vec<_>>>()?;
        ClipMixState::from_parts(revision, controls)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClipMixEntryWire {
    clip_id: ClipId,
    controls: ClipMixControlsWire,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClipMixControlsWire {
    input_layout: ChannelLayout,
    output_layout: ChannelLayout,
    channel_map: Vec<ChannelMapWire>,
    gain_bits: u32,
    fade_in_frames: String,
    fade_out_frames: String,
    pan_bits: u32,
    muted: bool,
    solo: bool,
    phase_inverted: Vec<ChannelPosition>,
}

impl ClipMixControlsWire {
    fn from_controls(controls: &ClipMixControls) -> Self {
        Self {
            input_layout: controls.input_layout().clone(),
            output_layout: controls.output_layout().clone(),
            channel_map: controls
                .channel_map()
                .iter()
                .copied()
                .map(ChannelMapWire::from_route)
                .collect(),
            gain_bits: controls.gain().to_bits(),
            fade_in_frames: controls.fade_in_frames().to_string(),
            fade_out_frames: controls.fade_out_frames().to_string(),
            pan_bits: controls.pan().to_bits(),
            muted: controls.muted(),
            solo: controls.solo(),
            phase_inverted: controls.phase_inverted().iter().copied().collect(),
        }
    }

    fn into_controls(self) -> Result<ClipMixControls> {
        if self.channel_map.len() > MAX_ROUTES_PER_CLIP {
            return Err(serialization_error(
                ErrorCategory::ResourceExhausted,
                "decode_clip_mix_controls",
                "clip-mix controls contain too many channel routes",
            ));
        }
        let routes = self
            .channel_map
            .into_iter()
            .map(ChannelMapWire::into_route)
            .collect::<Result<Vec<_>>>()?;
        let fade_in = parse_canonical_u64(&self.fade_in_frames, "fade-in frame count")?;
        let fade_out = parse_canonical_u64(&self.fade_out_frames, "fade-out frame count")?;
        let phase: BTreeSet<_> = self.phase_inverted.iter().copied().collect();
        if phase.len() != self.phase_inverted.len() {
            return Err(serialization_error(
                ErrorCategory::CorruptData,
                "decode_clip_mix_controls",
                "clip-mix phase set contains duplicate channels",
            ));
        }
        ClipMixControls::new(self.input_layout, self.output_layout, routes)?
            .with_gain(f32::from_bits(self.gain_bits))?
            .with_fades(fade_in, fade_out)?
            .with_pan(f32::from_bits(self.pan_bits))?
            .with_phase_inverted(phase)
            .map(|controls| controls.with_muted(self.muted).with_solo(self.solo))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChannelMapWire {
    source: ChannelPosition,
    destination: ChannelPosition,
    gain_bits: u32,
}

impl ChannelMapWire {
    fn from_route(route: ChannelMap) -> Self {
        Self {
            source: route.source(),
            destination: route.destination(),
            gain_bits: route.gain().to_bits(),
        }
    }

    fn into_route(self) -> Result<ChannelMap> {
        ChannelMap::new(
            self.source,
            self.destination,
            f32::from_bits(self.gain_bits),
        )
    }
}

fn parse_canonical_u64(value: &str, label: &str) -> Result<u64> {
    let parsed = value.parse::<u64>().map_err(|_| {
        serialization_error(
            ErrorCategory::CorruptData,
            "decode_clip_mix",
            format!("{label} is not a valid integer"),
        )
    })?;
    if value != parsed.to_string() {
        return Err(serialization_error(
            ErrorCategory::CorruptData,
            "decode_clip_mix",
            format!("{label} is not canonical"),
        ));
    }
    Ok(parsed)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn serialization_error(
    category: ErrorCategory,
    operation: &'static str,
    message: impl Into<String>,
) -> Error {
    Error::new(category, Recoverability::UserCorrectable, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
