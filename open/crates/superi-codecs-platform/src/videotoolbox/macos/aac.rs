//! AAC AudioConverter sessions.
#![allow(unsafe_code)]

use std::collections::VecDeque;
use std::ffi::c_void;
use std::mem;
use std::ptr::{self, NonNull};
use std::sync::Arc;

use objc2_audio_toolbox::{
    kAudioConverterCompressionMagicCookie, kAudioConverterDecompressionMagicCookie,
    AudioConverterDispose, AudioConverterFillComplexBuffer, AudioConverterGetProperty,
    AudioConverterGetPropertyInfo, AudioConverterNew, AudioConverterRef, AudioConverterReset,
    AudioConverterSetProperty,
};
use objc2_core_audio_types::{
    kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked, kAudioFormatFlagIsSignedInteger,
    kAudioFormatLinearPCM, kAudioFormatMPEG4AAC, AudioBuffer, AudioBufferList,
    AudioStreamBasicDescription, AudioStreamPacketDescription,
};
use superi_core::error::Result;
use superi_core::pixel::SampleFormat;
use superi_core::time::{SampleTime, TimeRounding, Timebase};
use superi_media_io::audio_io::{AudioBlock, AudioFormat, AudioPlane};
use superi_media_io::decode::{DecodeOutput, Decoder, DecoderConfig};
use superi_media_io::demux::{MetadataValue, Packet, PacketTiming, StreamKind};
use superi_media_io::encode::{
    EncodeInput, EncodeOutput, Encoder, EncoderConfig, EncoderMediaFormat,
};
use superi_media_io::operation::OperationContext;

use super::{check_status, conflict, corrupt, invalid, status_error, unsupported, AAC_CODEC_ID};

const AAC_FRAMES_PER_PACKET: u32 = 1024;
const OUTPUT_CAPACITY: usize = 1 << 20;
const MAX_OUTPUT_PACKETS: usize = 256;

pub(super) fn create_decoder(
    config: &DecoderConfig,
    _operation: &OperationContext,
) -> Result<Box<dyn Decoder>> {
    if config.stream().kind() != StreamKind::Audio
        || config.stream().codec().as_str() != AAC_CODEC_ID
    {
        return Err(unsupported(
            "create_aac_decoder",
            "an AAC decoder requires an AAC audio stream",
        ));
    }
    let format = config.audio_format().ok_or_else(|| {
        invalid(
            "create_aac_decoder",
            "AAC decoding requires an explicit output audio format",
        )
    })?;
    validate_packed_audio(format, "create_aac_decoder")?;
    let cookie = codec_cookie(config)?;
    let converter = AudioConverter::new(
        aac_description(format),
        pcm_description(format)?,
        Some((&cookie, kAudioConverterDecompressionMagicCookie)),
    )?;
    Ok(Box::new(AacDecoder {
        config: config.clone(),
        format: format.clone(),
        converter,
        output: VecDeque::new(),
        flushed: false,
    }))
}

pub(super) fn create_encoder(
    config: &EncoderConfig,
    _operation: &OperationContext,
) -> Result<Box<dyn Encoder>> {
    if config.codec().as_str() != AAC_CODEC_ID {
        return Err(unsupported(
            "create_aac_encoder",
            "an AAC encoder requires the AAC codec identifier",
        ));
    }
    let EncoderMediaFormat::Audio(format) = config.media_format() else {
        return Err(invalid(
            "create_aac_encoder",
            "an AAC encoder requires an audio media format",
        ));
    };
    validate_packed_audio(format, "create_aac_encoder")?;
    let converter = AudioConverter::new(pcm_description(format)?, aac_description(format), None)?;
    Ok(Box::new(AacEncoder {
        config: config.clone(),
        format: format.clone(),
        converter,
        output: VecDeque::new(),
        next_output_sample: None,
        flushed: false,
    }))
}

struct AudioConverter(AudioConverterRef);

unsafe impl Send for AudioConverter {}

impl AudioConverter {
    fn new(
        mut input: AudioStreamBasicDescription,
        mut output: AudioStreamBasicDescription,
        cookie: Option<(&[u8], u32)>,
    ) -> Result<Self> {
        let mut converter = ptr::null_mut();
        let status = unsafe {
            AudioConverterNew(
                NonNull::from(&mut input),
                NonNull::from(&mut output),
                NonNull::from(&mut converter),
            )
        };
        check_status(status, "create_audio_converter")?;
        if converter.is_null() {
            return Err(status_error(-50, "create_audio_converter"));
        }
        let value = Self(converter);
        if let Some((cookie, property)) = cookie {
            if cookie.is_empty() {
                return Err(corrupt(
                    "configure_audio_converter_cookie",
                    "AAC codec configuration must not be empty",
                ));
            }
            let size = u32::try_from(cookie.len()).map_err(|_| {
                corrupt(
                    "configure_audio_converter_cookie",
                    "AAC codec configuration exceeds macOS limits",
                )
            })?;
            let pointer = NonNull::new(cookie.as_ptr().cast_mut().cast::<c_void>())
                .expect("nonempty cookie has a nonnull pointer");
            let status = unsafe { AudioConverterSetProperty(value.0, property, size, pointer) };
            check_status(status, "configure_audio_converter_cookie")?;
        }
        Ok(value)
    }

    fn reset(&mut self) -> Result<()> {
        check_status(
            unsafe { AudioConverterReset(self.0) },
            "reset_audio_converter",
        )
    }

    fn compression_cookie(&self) -> Option<Vec<u8>> {
        let mut size = 0_u32;
        if unsafe {
            AudioConverterGetPropertyInfo(
                self.0,
                kAudioConverterCompressionMagicCookie,
                &mut size,
                ptr::null_mut(),
            )
        } != 0
            || size == 0
        {
            return None;
        }
        let mut bytes = vec![0_u8; usize::try_from(size).ok()?];
        let pointer = NonNull::new(bytes.as_mut_ptr().cast::<c_void>())?;
        if unsafe {
            AudioConverterGetProperty(
                self.0,
                kAudioConverterCompressionMagicCookie,
                NonNull::from(&mut size),
                pointer,
            )
        } != 0
        {
            return None;
        }
        bytes.truncate(usize::try_from(size).ok()?);
        Some(bytes)
    }
}

impl Drop for AudioConverter {
    fn drop(&mut self) {
        let _ = unsafe { AudioConverterDispose(self.0) };
    }
}

struct AacDecoder {
    config: DecoderConfig,
    format: AudioFormat,
    converter: AudioConverter,
    output: VecDeque<AudioBlock>,
    flushed: bool,
}

impl Decoder for AacDecoder {
    fn config(&self) -> &DecoderConfig {
        &self.config
    }

    fn send_packet(&mut self, packet: Packet, operation: &OperationContext) -> Result<()> {
        operation.check("send_aac_packet")?;
        if self.flushed {
            return Err(conflict(
                "send_aac_packet",
                "cannot send an AAC packet after flush without reset",
            ));
        }
        if packet.stream_id() != self.config.stream().id() {
            return Err(invalid(
                "send_aac_packet",
                "packet stream identity does not match the AAC decoder configuration",
            ));
        }
        if packet.data().is_empty() {
            return Err(corrupt("send_aac_packet", "AAC packet must not be empty"));
        }
        let mut context = InputContext::compressed(
            packet.data(),
            u32::try_from(self.format.channel_layout().len()).expect("validated channels"),
        );
        let mut output_bytes = vec![0_u8; OUTPUT_CAPACITY];
        let mut output_list = output_list(&mut output_bytes, self.format.channel_layout().len())?;
        let frame_capacity = output_bytes.len() / bytes_per_frame(&self.format)?;
        let mut output_packets = u32::try_from(frame_capacity).map_err(|_| {
            invalid(
                "decode_aac_packet",
                "AAC output capacity exceeds macOS limits",
            )
        })?;
        let status = unsafe {
            AudioConverterFillComplexBuffer(
                self.converter.0,
                Some(input_callback),
                ptr::from_mut(&mut context).cast(),
                NonNull::from(&mut output_packets),
                NonNull::from(&mut output_list),
                ptr::null_mut(),
            )
        };
        check_status(status, "decode_aac_packet")?;
        if output_packets != 0 {
            let byte_count = usize::try_from(output_packets)
                .ok()
                .and_then(|frames| frames.checked_mul(bytes_per_frame(&self.format).ok()?))
                .ok_or_else(|| invalid("decode_aac_packet", "decoded AAC size overflowed"))?;
            output_bytes.truncate(byte_count);
            let timestamp = packet
                .timing()
                .presentation_time()
                .map(|value| {
                    value.checked_rescale(
                        Timebase::integer(self.format.sample_rate())?,
                        TimeRounding::Exact,
                    )
                })
                .transpose()?
                .map(|value| value.value())
                .unwrap_or(0);
            let block = AudioBlock::new(
                self.format.clone(),
                SampleTime::new(timestamp, self.format.sample_rate())?,
                u64::from(output_packets),
                vec![AudioPlane::new(Arc::from(output_bytes))],
            )?
            .with_metadata(
                "platform.backend",
                MetadataValue::Text("apple-audio-converter".to_owned()),
            )?;
            self.output.push_back(block);
        }
        Ok(())
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<DecodeOutput> {
        operation.check("receive_aac_audio")?;
        if let Some(block) = self.output.pop_front() {
            Ok(DecodeOutput::Audio(block))
        } else if self.flushed {
            Ok(DecodeOutput::EndOfStream)
        } else {
            Ok(DecodeOutput::NeedInput)
        }
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("flush_aac_decoder")?;
        self.flushed = true;
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("reset_aac_decoder")?;
        self.converter.reset()?;
        self.output.clear();
        self.flushed = false;
        Ok(())
    }
}

struct AacEncoder {
    config: EncoderConfig,
    format: AudioFormat,
    converter: AudioConverter,
    output: VecDeque<Packet>,
    next_output_sample: Option<i64>,
    flushed: bool,
}

impl AacEncoder {
    fn convert(&mut self, bytes: &[u8], frames: u32) -> Result<()> {
        let mut context = InputContext::pcm(
            bytes,
            frames,
            bytes_per_frame(&self.format)?,
            u32::try_from(self.format.channel_layout().len()).expect("validated channels"),
        );
        loop {
            let mut output_bytes = vec![0_u8; OUTPUT_CAPACITY];
            let mut output_list =
                output_list(&mut output_bytes, self.format.channel_layout().len())?;
            let mut descriptions = vec![
                AudioStreamPacketDescription {
                    mStartOffset: 0,
                    mVariableFramesInPacket: 0,
                    mDataByteSize: 0,
                };
                MAX_OUTPUT_PACKETS
            ];
            let mut output_packets =
                u32::try_from(MAX_OUTPUT_PACKETS).expect("packet bound fits u32");
            let status = unsafe {
                AudioConverterFillComplexBuffer(
                    self.converter.0,
                    Some(input_callback),
                    ptr::from_mut(&mut context).cast(),
                    NonNull::from(&mut output_packets),
                    NonNull::from(&mut output_list),
                    descriptions.as_mut_ptr(),
                )
            };
            check_status(status, "encode_aac_audio")?;
            self.queue_encoded_packets(
                &output_bytes,
                &descriptions[..usize::try_from(output_packets).expect("u32 fits usize")],
            )?;
            if output_packets == 0
                || (context.remaining_packets == 0
                    && usize::try_from(output_packets).expect("u32 fits usize")
                        < MAX_OUTPUT_PACKETS)
            {
                break;
            }
        }
        Ok(())
    }

    fn queue_encoded_packets(
        &mut self,
        bytes: &[u8],
        descriptions: &[AudioStreamPacketDescription],
    ) -> Result<()> {
        let cookie = self.converter.compression_cookie();
        for description in descriptions {
            let start = usize::try_from(description.mStartOffset).map_err(|_| {
                corrupt(
                    "encode_aac_audio",
                    "AudioConverter returned a negative packet offset",
                )
            })?;
            let size = usize::try_from(description.mDataByteSize).expect("u32 fits usize");
            let end = start
                .checked_add(size)
                .filter(|end| *end <= bytes.len())
                .ok_or_else(|| {
                    corrupt("encode_aac_audio", "AudioConverter packet range is invalid")
                })?;
            let timestamp = self.next_output_sample.unwrap_or(0);
            let duration = if description.mVariableFramesInPacket == 0 {
                AAC_FRAMES_PER_PACKET
            } else {
                description.mVariableFramesInPacket
            };
            let timing = PacketTiming::new(
                self.config.timebase(),
                Some(timestamp),
                Some(timestamp),
                Some(u64::from(duration)),
            )?;
            let mut packet = Packet::new(
                self.config.stream_id(),
                Arc::from(bytes[start..end].to_vec()),
                timing,
            )
            .with_keyframe(true);
            if let Some(cookie) = cookie.as_ref() {
                packet = packet.with_metadata(
                    "codec.configuration",
                    MetadataValue::Bytes(Arc::from(cookie.clone())),
                )?;
            }
            self.output.push_back(packet);
            self.next_output_sample = Some(timestamp + i64::from(duration));
        }
        Ok(())
    }
}

impl Encoder for AacEncoder {
    fn config(&self) -> &EncoderConfig {
        &self.config
    }

    fn send(&mut self, input: EncodeInput, operation: &OperationContext) -> Result<()> {
        operation.check("send_aac_audio")?;
        if self.flushed {
            return Err(conflict(
                "send_aac_audio",
                "cannot send AAC audio after flush without reset",
            ));
        }
        let EncodeInput::Audio(block) = input else {
            return Err(invalid(
                "send_aac_audio",
                "an AAC encoder requires audio blocks",
            ));
        };
        if block.format() != &self.format {
            return Err(invalid(
                "send_aac_audio",
                "audio block format does not match the AAC encoder configuration",
            ));
        }
        if self.next_output_sample.is_none() {
            self.next_output_sample = Some(block.timestamp().sample());
        }
        let frames = u32::try_from(block.frame_count()).map_err(|_| {
            invalid(
                "send_aac_audio",
                "audio block exceeds AudioConverter limits",
            )
        })?;
        self.convert(block.planes()[0].bytes(), frames)
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<EncodeOutput> {
        operation.check("receive_aac_packet")?;
        if let Some(packet) = self.output.pop_front() {
            Ok(EncodeOutput::Packet(packet))
        } else if self.flushed {
            Ok(EncodeOutput::EndOfStream)
        } else {
            Ok(EncodeOutput::NeedInput)
        }
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("flush_aac_encoder")?;
        if !self.flushed {
            self.convert(&[], 0)?;
            self.flushed = true;
        }
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("reset_aac_encoder")?;
        self.converter.reset()?;
        self.output.clear();
        self.next_output_sample = None;
        self.flushed = false;
        Ok(())
    }
}

struct InputContext<'a> {
    bytes: &'a [u8],
    byte_offset: usize,
    remaining_packets: u32,
    bytes_per_packet: usize,
    compressed: bool,
    channels: u32,
    packet_description: AudioStreamPacketDescription,
}

impl<'a> InputContext<'a> {
    fn pcm(bytes: &'a [u8], frames: u32, bytes_per_frame: usize, channels: u32) -> Self {
        Self {
            bytes,
            byte_offset: 0,
            remaining_packets: frames,
            bytes_per_packet: bytes_per_frame,
            compressed: false,
            channels,
            packet_description: AudioStreamPacketDescription {
                mStartOffset: 0,
                mVariableFramesInPacket: 0,
                mDataByteSize: 0,
            },
        }
    }

    fn compressed(bytes: &'a [u8], channels: u32) -> Self {
        Self {
            bytes,
            byte_offset: 0,
            remaining_packets: 1,
            bytes_per_packet: bytes.len(),
            compressed: true,
            channels,
            packet_description: AudioStreamPacketDescription {
                mStartOffset: 0,
                mVariableFramesInPacket: AAC_FRAMES_PER_PACKET,
                mDataByteSize: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
            },
        }
    }
}

unsafe extern "C-unwind" fn input_callback(
    _converter: AudioConverterRef,
    mut requested_packets: NonNull<u32>,
    mut data: NonNull<AudioBufferList>,
    packet_description: *mut *mut AudioStreamPacketDescription,
    context: *mut c_void,
) -> i32 {
    let context = unsafe { &mut *context.cast::<InputContext<'_>>() };
    let requested = unsafe { *requested_packets.as_ref() };
    let supplied = requested.min(context.remaining_packets);
    unsafe { *requested_packets.as_mut() = supplied };
    let list = unsafe { data.as_mut() };
    list.mNumberBuffers = 1;
    if supplied == 0 {
        list.mBuffers[0] = AudioBuffer {
            mNumberChannels: 0,
            mDataByteSize: 0,
            mData: ptr::null_mut(),
        };
        return 0;
    }
    let byte_count = usize::try_from(supplied)
        .ok()
        .and_then(|packets| packets.checked_mul(context.bytes_per_packet))
        .unwrap_or(0)
        .min(context.bytes.len().saturating_sub(context.byte_offset));
    let pointer = unsafe { context.bytes.as_ptr().add(context.byte_offset) }
        .cast_mut()
        .cast();
    list.mBuffers[0] = AudioBuffer {
        mNumberChannels: context.channels,
        mDataByteSize: u32::try_from(byte_count).unwrap_or(u32::MAX),
        mData: pointer,
    };
    if context.compressed && !packet_description.is_null() {
        unsafe { *packet_description = ptr::from_mut(&mut context.packet_description) };
    }
    context.byte_offset += byte_count;
    context.remaining_packets -= supplied;
    0
}

fn output_list(bytes: &mut [u8], channels: usize) -> Result<AudioBufferList> {
    Ok(AudioBufferList {
        mNumberBuffers: 1,
        mBuffers: [AudioBuffer {
            mNumberChannels: u32::try_from(channels).map_err(|_| {
                invalid(
                    "create_audio_buffer_list",
                    "channel count exceeds macOS limits",
                )
            })?,
            mDataByteSize: u32::try_from(bytes.len()).map_err(|_| {
                invalid(
                    "create_audio_buffer_list",
                    "audio buffer exceeds macOS limits",
                )
            })?,
            mData: bytes.as_mut_ptr().cast(),
        }],
    })
}

fn validate_packed_audio(format: &AudioFormat, operation: &'static str) -> Result<()> {
    if format.sample_format().is_planar() {
        return Err(unsupported(
            operation,
            "AAC AudioConverter sessions require packed PCM audio",
        ));
    }
    let channels = format.channel_layout().len();
    if channels > 8 {
        return Err(unsupported(
            operation,
            "AAC AudioConverter sessions support at most eight channels",
        ));
    }
    Ok(())
}

fn bytes_per_frame(format: &AudioFormat) -> Result<usize> {
    usize::from(format.sample_format().bytes_per_sample())
        .checked_mul(format.channel_layout().len())
        .ok_or_else(|| invalid("describe_pcm_audio", "PCM frame size overflowed"))
}

fn pcm_description(format: &AudioFormat) -> Result<AudioStreamBasicDescription> {
    let bytes_per_frame = u32::try_from(bytes_per_frame(format)?)
        .map_err(|_| invalid("describe_pcm_audio", "PCM frame size exceeds macOS limits"))?;
    let sample_format = format.sample_format();
    let mut flags = kAudioFormatFlagIsPacked;
    flags |= match sample_format {
        SampleFormat::F32 | SampleFormat::F64 => kAudioFormatFlagIsFloat,
        SampleFormat::I16 | SampleFormat::I24 | SampleFormat::I32 => {
            kAudioFormatFlagIsSignedInteger
        }
        SampleFormat::U8 => 0,
        _ => {
            return Err(unsupported(
                "describe_pcm_audio",
                "planar PCM is not supported by this AAC binding",
            ))
        }
    };
    Ok(AudioStreamBasicDescription {
        mSampleRate: f64::from(format.sample_rate()),
        mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: flags,
        mBytesPerPacket: bytes_per_frame,
        mFramesPerPacket: 1,
        mBytesPerFrame: bytes_per_frame,
        mChannelsPerFrame: u32::try_from(format.channel_layout().len())
            .map_err(|_| invalid("describe_pcm_audio", "channel count exceeds macOS limits"))?,
        mBitsPerChannel: u32::from(sample_format.bits_per_sample()),
        mReserved: 0,
    })
}

fn aac_description(format: &AudioFormat) -> AudioStreamBasicDescription {
    AudioStreamBasicDescription {
        mSampleRate: f64::from(format.sample_rate()),
        mFormatID: kAudioFormatMPEG4AAC,
        mFormatFlags: 0,
        mBytesPerPacket: 0,
        mFramesPerPacket: AAC_FRAMES_PER_PACKET,
        mBytesPerFrame: 0,
        mChannelsPerFrame: u32::try_from(format.channel_layout().len())
            .expect("validated channels"),
        mBitsPerChannel: 0,
        mReserved: 0,
    }
}

fn codec_cookie(config: &DecoderConfig) -> Result<Vec<u8>> {
    let bytes = match config.stream().metadata().get("codec.configuration") {
        Some(MetadataValue::Bytes(bytes)) => bytes.as_ref(),
        Some(_) => {
            return Err(corrupt(
                "read_aac_codec_configuration",
                "AAC codec.configuration metadata must contain bytes",
            ))
        }
        None => {
            return Err(corrupt(
                "read_aac_codec_configuration",
                "AAC streams require codec.configuration metadata",
            ))
        }
    };
    if valid_audio_specific_config(bytes) {
        return make_esds_cookie(bytes);
    }
    extract_audio_specific_config(bytes)?;
    Ok(bytes.to_vec())
}

fn make_esds_cookie(audio_specific_config: &[u8]) -> Result<Vec<u8>> {
    let asc_length = u8::try_from(audio_specific_config.len()).map_err(|_| {
        corrupt(
            "build_aac_esds",
            "AAC AudioSpecificConfig exceeds the supported size",
        )
    })?;
    let decoder_length = asc_length
        .checked_add(18)
        .ok_or_else(|| corrupt("build_aac_esds", "AAC decoder descriptor length overflowed"))?;
    let es_length = asc_length.checked_add(32).ok_or_else(|| {
        corrupt(
            "build_aac_esds",
            "AAC elementary stream descriptor length overflowed",
        )
    })?;
    let mut cookie = vec![
        0x03,
        0x80,
        0x80,
        0x80,
        es_length,
        0,
        0,
        0,
        0x04,
        0x80,
        0x80,
        0x80,
        decoder_length,
        0x40,
        0x14,
        0,
        0x18,
        0,
        0,
        0,
        0,
        0,
        0,
        1,
        0xf4,
        0,
        0x05,
        0x80,
        0x80,
        0x80,
        asc_length,
    ];
    cookie.extend_from_slice(audio_specific_config);
    cookie.extend_from_slice(&[0x06, 0x80, 0x80, 0x80, 1, 2]);
    Ok(cookie)
}

fn extract_audio_specific_config(bytes: &[u8]) -> Result<Vec<u8>> {
    if valid_audio_specific_config(bytes) {
        return Ok(bytes.to_vec());
    }
    let mut offset = 0;
    while offset < bytes.len() {
        if bytes[offset] == 0x05 {
            let mut length = 0_usize;
            let mut cursor = offset + 1;
            for _ in 0..4 {
                let byte = *bytes.get(cursor).ok_or_else(|| {
                    corrupt("parse_aac_esds", "AAC descriptor length is truncated")
                })?;
                cursor += 1;
                length = (length << 7) | usize::from(byte & 0x7f);
                if byte & 0x80 == 0 {
                    let end = cursor
                        .checked_add(length)
                        .filter(|end| *end <= bytes.len())
                        .ok_or_else(|| {
                            corrupt("parse_aac_esds", "AAC descriptor payload is truncated")
                        })?;
                    let value = &bytes[cursor..end];
                    if valid_audio_specific_config(value) {
                        return Ok(value.to_vec());
                    }
                    break;
                }
            }
        }
        offset += 1;
    }
    Err(corrupt(
        "parse_aac_esds",
        "AAC configuration does not contain a valid AudioSpecificConfig",
    ))
}

fn valid_audio_specific_config(bytes: &[u8]) -> bool {
    if bytes.len() < 2 {
        return false;
    }
    let object_type = bytes[0] >> 3;
    let frequency_index = ((bytes[0] & 0x07) << 1) | (bytes[1] >> 7);
    let channel_configuration = (bytes[1] >> 3) & 0x0f;
    object_type != 0 && object_type != 31 && frequency_index != 15 && channel_configuration != 0
}

const _: () = assert!(mem::size_of::<AudioBufferList>() >= mem::size_of::<AudioBuffer>());
