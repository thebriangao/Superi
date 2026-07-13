//! Vorbis I audio decode and encode.
//!
//! Decode uses lewton's packet-level pure Rust implementation. Encode uses the safe
//! `vorbis_rs` wrapper around bundled permissive libvorbis code, then removes its Ogg transport
//! so callers receive raw Vorbis packets through the codec-neutral media interface. Standard
//! Vorbis channel order is translated by semantic channel position at both boundaries.

use std::collections::VecDeque;
use std::fmt;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::num::{NonZeroU32, NonZeroU8};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

// Direct sys imports keep transitive resolution on the Rust 1.80-compatible releases.
use aotuv_lancer_vorbis_sys as _;
use lewton::audio::{get_decoded_sample_count, read_audio_packet_generic, PreviousWindowRight};
use lewton::header::{
    read_header_comment, read_header_ident, read_header_setup, IdentHeader, SetupHeader,
};
use ogg::PacketReader as OggPacketReader;
use ogg_next_sys as _;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::{ChannelLayout, ChannelPosition, SampleFormat};
use superi_core::time::{SampleTime, TimeRounding, Timebase};
use superi_media_io::audio_io::{AudioBlock, AudioFormat, AudioPlane};
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, BackendDescriptor, BackendRegistration, BackendTier,
    MediaBackend,
};
use superi_media_io::decode::{DecodeOutput, Decoder, DecoderConfig};
use superi_media_io::demux::{
    BackendId, CodecId, MediaMetadata, MediaSource, MetadataValue, Packet, PacketTiming,
    SourceProbe, SourceProbeResult, SourceRequest, StreamKind,
};
use superi_media_io::encode::{
    EncodeInput, EncodeOutput, Encoder, EncoderConfig, EncoderMediaFormat,
};
use superi_media_io::operation::OperationContext;
use vorbis_rs::VorbisEncoderBuilder;

/// Stable codec identifier used by streams, registry selection, and capability introspection.
pub const VORBIS_CODEC_ID: &str = "vorbis";

const HEADER_TYPES: [u8; 3] = [1, 3, 5];
const HEADER_NAMES: [&str; 3] = ["identification", "comment", "setup"];
const UNKNOWN_GRANULE_POSITION: u64 = u64::MAX;

const MONO_ORDER: [ChannelPosition; 1] = [ChannelPosition::FrontCenter];
const STEREO_ORDER: [ChannelPosition; 2] =
    [ChannelPosition::FrontLeft, ChannelPosition::FrontRight];
const THREE_CHANNEL_ORDER: [ChannelPosition; 3] = [
    ChannelPosition::FrontLeft,
    ChannelPosition::FrontCenter,
    ChannelPosition::FrontRight,
];
const QUAD_ORDER: [ChannelPosition; 4] = [
    ChannelPosition::FrontLeft,
    ChannelPosition::FrontRight,
    ChannelPosition::BackLeft,
    ChannelPosition::BackRight,
];
const FIVE_CHANNEL_ORDER: [ChannelPosition; 5] = [
    ChannelPosition::FrontLeft,
    ChannelPosition::FrontCenter,
    ChannelPosition::FrontRight,
    ChannelPosition::BackLeft,
    ChannelPosition::BackRight,
];
const FIVE_ONE_ORDER: [ChannelPosition; 6] = [
    ChannelPosition::FrontLeft,
    ChannelPosition::FrontCenter,
    ChannelPosition::FrontRight,
    ChannelPosition::BackLeft,
    ChannelPosition::BackRight,
    ChannelPosition::LowFrequency,
];
const SIX_ONE_ORDER: [ChannelPosition; 7] = [
    ChannelPosition::FrontLeft,
    ChannelPosition::FrontCenter,
    ChannelPosition::FrontRight,
    ChannelPosition::SideLeft,
    ChannelPosition::SideRight,
    ChannelPosition::BackCenter,
    ChannelPosition::LowFrequency,
];
const SEVEN_ONE_ORDER: [ChannelPosition; 8] = [
    ChannelPosition::FrontLeft,
    ChannelPosition::FrontCenter,
    ChannelPosition::FrontRight,
    ChannelPosition::SideLeft,
    ChannelPosition::SideRight,
    ChannelPosition::BackLeft,
    ChannelPosition::BackRight,
    ChannelPosition::LowFrequency,
];

/// Default permissive Vorbis backend.
pub struct VorbisBackend {
    descriptor: BackendDescriptor,
}

impl VorbisBackend {
    /// Creates the backend with its stable identity.
    pub fn new() -> Result<Self> {
        Ok(Self {
            descriptor: BackendDescriptor::new(BackendId::new("rust-vorbis")?, "Rust Vorbis")?,
        })
    }

    /// Builds the primary registration for Vorbis decode and encode.
    pub fn registration() -> Result<BackendRegistration> {
        let codec = CodecId::new(VORBIS_CODEC_ID)?;
        BackendRegistration::new(
            Arc::new(Self::new()?),
            BackendCapabilities::new([
                BackendCapability::Decode(codec.clone()),
                BackendCapability::Encode(codec),
            ]),
            100,
            BackendTier::Primary,
        )
    }
}

impl MediaBackend for VorbisBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn probe_source(
        &self,
        _probe: &SourceProbe<'_>,
        operation: &OperationContext,
    ) -> Result<SourceProbeResult> {
        operation.check("probe_vorbis_source")?;
        Ok(SourceProbeResult::NoMatch)
    }

    fn open_source(
        &self,
        _request: &SourceRequest,
        operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>> {
        operation.check("open_vorbis_source")?;
        Err(unsupported(
            "open_vorbis_source",
            "the Vorbis codec backend does not open media containers",
        ))
    }

    fn create_decoder(
        &self,
        config: &DecoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Decoder>> {
        operation.check("create_vorbis_decoder")?;
        Ok(Box::new(VorbisDecoder::new(config.clone())?))
    }

    fn create_encoder(
        &self,
        config: &EncoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>> {
        operation.check("create_vorbis_encoder")?;
        Ok(Box::new(VorbisEncoder::new(config.clone())?))
    }
}

struct VorbisHeaders {
    ident: IdentHeader,
    setup: SetupHeader,
    format: AudioFormat,
    canonical_to_vorbis: Vec<usize>,
}

impl VorbisHeaders {
    fn parse(packets: &[Arc<[u8]>; 3]) -> Result<Self> {
        validate_header_packet(&packets[0], HEADER_TYPES[0], "parse_identification_header")?;
        validate_header_packet(&packets[1], HEADER_TYPES[1], "parse_comment_header")?;
        validate_header_packet(&packets[2], HEADER_TYPES[2], "parse_setup_header")?;

        let ident = read_header_ident(&packets[0]).map_err(|source| {
            corrupt_source(
                "parse_identification_header",
                "Vorbis identification header is corrupt",
                source,
            )
        })?;
        read_header_comment(&packets[1]).map_err(|source| {
            corrupt_source(
                "parse_comment_header",
                "Vorbis comment header is corrupt",
                source,
            )
        })?;
        let setup = read_header_setup(
            &packets[2],
            ident.audio_channels,
            (ident.blocksize_0, ident.blocksize_1),
        )
        .map_err(|source| {
            corrupt_source(
                "parse_setup_header",
                "Vorbis setup header is corrupt",
                source,
            )
        })?;
        let layout = canonical_layout(usize::from(ident.audio_channels))?;
        let canonical_to_vorbis = canonical_to_vorbis(&layout)?;
        let format = AudioFormat::new(ident.audio_sample_rate, SampleFormat::F32Planar, layout)?;
        Ok(Self {
            ident,
            setup,
            format,
            canonical_to_vorbis,
        })
    }
}

struct VorbisDecoder {
    config: DecoderConfig,
    configured_packets: Option<[Arc<[u8]>; 3]>,
    pending_headers: Vec<Arc<[u8]>>,
    headers: Option<VorbisHeaders>,
    previous_window: PreviousWindowRight,
    output: VecDeque<AudioBlock>,
    next_sample: Option<i64>,
    flushed: bool,
}

impl VorbisDecoder {
    fn new(config: DecoderConfig) -> Result<Self> {
        validate_decoder_stream(&config)?;
        let configured_packets = configured_header_packets(&config)?;
        let headers = configured_packets
            .as_ref()
            .map(VorbisHeaders::parse)
            .transpose()?;
        if let Some(headers) = headers.as_ref() {
            validate_requested_audio_format(&config, &headers.format)?;
        }
        Ok(Self {
            config,
            configured_packets,
            pending_headers: Vec::new(),
            headers,
            previous_window: PreviousWindowRight::new(),
            output: VecDeque::new(),
            next_sample: None,
            flushed: false,
        })
    }

    fn accept_header(&mut self, packet: &Packet) -> Result<bool> {
        let Some(first) = packet.data().first().copied() else {
            return Err(corrupt("decode_vorbis_packet", "Vorbis packet is empty"));
        };
        if first & 1 == 0 {
            return Ok(false);
        }
        if self.headers.is_some() {
            return Err(corrupt(
                "decode_vorbis_header",
                "Vorbis headers were repeated after decoder initialization",
            ));
        }
        let index = self.pending_headers.len();
        let Some(expected) = HEADER_TYPES.get(index).copied() else {
            return Err(corrupt(
                "decode_vorbis_header",
                "Vorbis stream contains too many header packets",
            ));
        };
        validate_header_packet(packet.data(), expected, "decode_vorbis_header")?;
        self.pending_headers.push(Arc::from(packet.data()));
        if self.pending_headers.len() == 3 {
            let packets: [Arc<[u8]>; 3] = self
                .pending_headers
                .clone()
                .try_into()
                .expect("three Vorbis headers were collected");
            let headers = VorbisHeaders::parse(&packets)?;
            validate_requested_audio_format(&self.config, &headers.format)?;
            self.headers = Some(headers);
        }
        Ok(true)
    }

    fn decode_audio_packet(&mut self, packet: &Packet) -> Result<()> {
        let headers = self.headers.as_ref().ok_or_else(|| {
            corrupt(
                "decode_vorbis_packet",
                "Vorbis audio arrived before all three headers",
            )
        })?;
        let mut decoded: Vec<Vec<f32>> = read_audio_packet_generic(
            &headers.ident,
            &headers.setup,
            packet.data(),
            &mut self.previous_window,
        )
        .map_err(|source| {
            corrupt_source(
                "decode_vorbis_packet",
                "Vorbis audio packet is corrupt",
                source,
            )
        })?;
        let mut frame_count = decoded.first().map_or(0, Vec::len);
        if decoded.len() != headers.format.channel_layout().len()
            || decoded.iter().any(|channel| channel.len() != frame_count)
        {
            return Err(internal(
                "decode_vorbis_packet",
                "Vorbis decoder returned inconsistent channel planes",
            ));
        }

        if let Some(duration) = packet_sample_duration(packet, headers.format.sample_rate())? {
            let requested = usize::try_from(duration).map_err(|_| {
                corrupt(
                    "decode_vorbis_packet",
                    "Vorbis packet duration cannot be represented on this platform",
                )
            })?;
            if requested > frame_count {
                return Err(corrupt(
                    "decode_vorbis_packet",
                    "Vorbis packet duration exceeds decoded sample output",
                ));
            }
            decoded
                .iter_mut()
                .for_each(|channel| channel.truncate(requested));
            frame_count = requested;
        }

        let timestamp =
            packet_sample_timestamp(packet, headers.format.sample_rate(), self.next_sample)?;
        let frame_count_u64 = u64::try_from(frame_count).map_err(|_| {
            corrupt(
                "decode_vorbis_packet",
                "Vorbis sample count cannot be represented",
            )
        })?;
        let next = timestamp
            .sample()
            .checked_add(
                i64::try_from(frame_count_u64).map_err(|_| {
                    corrupt("decode_vorbis_packet", "Vorbis sample cursor overflowed")
                })?,
            )
            .ok_or_else(|| corrupt("decode_vorbis_packet", "Vorbis sample cursor overflowed"))?;
        self.next_sample = Some(next);
        if frame_count == 0 {
            return Ok(());
        }

        let planes = headers
            .canonical_to_vorbis
            .iter()
            .map(|&source| {
                let bytes = decoded[source]
                    .iter()
                    .flat_map(|sample| sample.to_le_bytes())
                    .collect::<Vec<_>>();
                AudioPlane::new(Arc::from(bytes))
            })
            .collect::<Vec<_>>();
        let mut block =
            AudioBlock::new(headers.format.clone(), timestamp, frame_count_u64, planes)?;
        for (key, value) in packet.metadata().iter() {
            block = block.with_metadata(key, value.clone())?;
        }
        self.output.push_back(block);
        Ok(())
    }

    fn restore_initial_state(&mut self) -> Result<()> {
        self.pending_headers.clear();
        self.headers = self
            .configured_packets
            .as_ref()
            .map(VorbisHeaders::parse)
            .transpose()?;
        self.previous_window = PreviousWindowRight::new();
        self.output.clear();
        self.next_sample = None;
        self.flushed = false;
        Ok(())
    }
}

impl Decoder for VorbisDecoder {
    fn config(&self) -> &DecoderConfig {
        &self.config
    }

    fn send_packet(&mut self, packet: Packet, operation: &OperationContext) -> Result<()> {
        operation.check("send_vorbis_packet")?;
        if self.flushed {
            return Err(conflict(
                "send_vorbis_packet",
                "cannot send Vorbis packets after flush without reset",
            ));
        }
        if packet.stream_id() != self.config.stream().id() {
            return Err(conflict(
                "send_vorbis_packet",
                "Vorbis packet belongs to a different stream",
            ));
        }
        if packet.timing().timebase() != self.config.stream().timebase() {
            return Err(corrupt(
                "send_vorbis_packet",
                "Vorbis packet timebase does not match its stream",
            ));
        }
        if !self.accept_header(&packet)? {
            self.decode_audio_packet(&packet)?;
        }
        operation.check("send_vorbis_packet")
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<DecodeOutput> {
        operation.check("receive_vorbis_audio")?;
        if let Some(block) = self.output.pop_front() {
            return Ok(DecodeOutput::Audio(block));
        }
        if self.flushed {
            Ok(DecodeOutput::EndOfStream)
        } else {
            Ok(DecodeOutput::NeedInput)
        }
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("flush_vorbis_decoder")?;
        if self.headers.is_none() && !self.pending_headers.is_empty() {
            return Err(corrupt(
                "flush_vorbis_decoder",
                "Vorbis stream ended before all three headers arrived",
            ));
        }
        self.flushed = true;
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("reset_vorbis_decoder")?;
        self.restore_initial_state()
    }
}

#[derive(Clone, Default)]
struct SharedOggBuffer {
    bytes: Arc<Mutex<Vec<u8>>>,
    position: u64,
}

impl Read for SharedOggBuffer {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let bytes = self
            .bytes
            .lock()
            .map_err(|_| io::Error::other("shared Ogg buffer lock was poisoned"))?;
        let position = usize::try_from(self.position)
            .map_err(|_| io::Error::other("shared Ogg read position overflowed"))?;
        if position >= bytes.len() {
            return Ok(0);
        }
        let count = output.len().min(bytes.len() - position);
        output[..count].copy_from_slice(&bytes[position..position + count]);
        self.position = self
            .position
            .checked_add(count as u64)
            .ok_or_else(|| io::Error::other("shared Ogg read position overflowed"))?;
        Ok(count)
    }
}

impl Seek for SharedOggBuffer {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let length = self
            .bytes
            .lock()
            .map_err(|_| io::Error::other("shared Ogg buffer lock was poisoned"))?
            .len() as i128;
        let next = match position {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::End(value) => length + i128::from(value),
            SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
        };
        if !(0..=i128::from(u64::MAX)).contains(&next) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "shared Ogg seek is outside the supported range",
            ));
        }
        self.position = next as u64;
        Ok(self.position)
    }
}

impl Write for SharedOggBuffer {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        self.bytes
            .lock()
            .map_err(|_| io::Error::other("shared Ogg buffer lock was poisoned"))?
            .extend_from_slice(input);
        Ok(input.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

type WorkerResult<T> = std::result::Result<T, EncoderWorkerError>;
type WorkerReply = SyncSender<WorkerResult<()>>;

#[derive(Debug)]
struct EncoderWorkerError(String);

impl fmt::Display for EncoderWorkerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for EncoderWorkerError {}

enum EncoderWorkerCommand {
    Encode {
        samples: Vec<Vec<f32>>,
        reply: WorkerReply,
    },
    Finish {
        reply: WorkerReply,
    },
    Stop,
}

struct VorbisEncoderWorker {
    commands: SyncSender<EncoderWorkerCommand>,
    thread: Option<JoinHandle<()>>,
}

impl VorbisEncoderWorker {
    fn start(
        sample_rate: NonZeroU32,
        channels: NonZeroU8,
        serial: i32,
        sink: SharedOggBuffer,
    ) -> Result<Self> {
        let (commands, receiver) = sync_channel(0);
        let (ready, ready_receiver) = sync_channel(0);
        let worker_thread = thread::Builder::new()
            .name("superi-vorbis-encoder".to_owned())
            .spawn(move || {
                encoder_worker_loop(sample_rate, channels, serial, sink, receiver, ready)
            })
            .map_err(|source| {
                internal_source(
                    "create_vorbis_encoder_worker",
                    "failed to start the Vorbis encoder worker",
                    source,
                )
            })?;
        match ready_receiver.recv() {
            Ok(Ok(())) => Ok(Self {
                commands,
                thread: Some(worker_thread),
            }),
            Ok(Err(source)) => {
                let _ = worker_thread.join();
                Err(invalid_source(
                    "create_vorbis_encoder",
                    "Vorbis encoder configuration is unsupported",
                    source,
                ))
            }
            Err(source) => {
                let _ = worker_thread.join();
                Err(internal_source(
                    "create_vorbis_encoder_worker",
                    "Vorbis encoder worker stopped during initialization",
                    source,
                ))
            }
        }
    }

    fn encode(&self, samples: Vec<Vec<f32>>) -> WorkerResult<()> {
        let (reply, receiver) = sync_channel(0);
        self.commands
            .send(EncoderWorkerCommand::Encode { samples, reply })
            .map_err(|_| EncoderWorkerError("Vorbis encoder worker is unavailable".to_owned()))?;
        receiver.recv().map_err(|_| {
            EncoderWorkerError("Vorbis encoder worker returned no result".to_owned())
        })?
    }

    fn finish(mut self) -> WorkerResult<()> {
        let (reply, receiver) = sync_channel(0);
        let result = self
            .commands
            .send(EncoderWorkerCommand::Finish { reply })
            .map_err(|_| EncoderWorkerError("Vorbis encoder worker is unavailable".to_owned()))
            .and_then(|()| {
                receiver.recv().map_err(|_| {
                    EncoderWorkerError("Vorbis encoder worker returned no result".to_owned())
                })?
            });
        let joined = self
            .thread
            .take()
            .expect("Vorbis worker thread is owned until finish")
            .join()
            .map_err(|_| EncoderWorkerError("Vorbis encoder worker panicked".to_owned()));
        result.and(joined)
    }

    fn stop(&mut self) {
        let _ = self.commands.send(EncoderWorkerCommand::Stop);
        if let Some(worker_thread) = self.thread.take() {
            let _ = worker_thread.join();
        }
    }
}

impl Drop for VorbisEncoderWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

fn encoder_worker_loop(
    sample_rate: NonZeroU32,
    channels: NonZeroU8,
    serial: i32,
    sink: SharedOggBuffer,
    commands: Receiver<EncoderWorkerCommand>,
    ready: WorkerReply,
) {
    let mut builder = VorbisEncoderBuilder::new_with_serial(sample_rate, channels, sink, serial);
    let mut library = match builder.build() {
        Ok(library) => {
            let _ = ready.send(Ok(()));
            library
        }
        Err(error) => {
            let _ = ready.send(Err(EncoderWorkerError(error.to_string())));
            return;
        }
    };
    while let Ok(command) = commands.recv() {
        match command {
            EncoderWorkerCommand::Encode { samples, reply } => {
                let result = library
                    .encode_audio_block(&samples)
                    .map_err(|error| EncoderWorkerError(error.to_string()));
                let _ = reply.send(result);
            }
            EncoderWorkerCommand::Finish { reply } => {
                let result = library
                    .finish()
                    .map(|_| ())
                    .map_err(|error| EncoderWorkerError(error.to_string()));
                let _ = reply.send(result);
                return;
            }
            EncoderWorkerCommand::Stop => return,
        }
    }
}

struct VorbisEncoder {
    config: EncoderConfig,
    format: AudioFormat,
    vorbis_source_channels: Vec<usize>,
    worker: Option<VorbisEncoderWorker>,
    packet_reader: OggPacketReader<SharedOggBuffer>,
    output: VecDeque<Packet>,
    header_packets: Vec<Arc<[u8]>>,
    timing_headers: Option<VorbisHeaders>,
    audio_packet_count: u64,
    relative_sample_cursor: u64,
    input_frame_count: u64,
    timeline_origin: Option<i64>,
    input_metadata: Vec<(u64, u64, MediaMetadata)>,
    flushed: bool,
}

impl VorbisEncoder {
    fn new(config: EncoderConfig) -> Result<Self> {
        if config.codec().as_str() != VORBIS_CODEC_ID {
            return Err(unsupported(
                "create_vorbis_encoder",
                "the requested codec is not Vorbis",
            ));
        }
        let EncoderMediaFormat::Audio(format) = config.media_format() else {
            return Err(invalid(
                "create_vorbis_encoder",
                "Vorbis encoding requires an audio format",
            ));
        };
        let format = format.clone();
        let vorbis_source_channels = vorbis_source_channels(format.channel_layout())?;
        let (worker, packet_reader) = create_encoder_worker(&config, &format)?;
        let mut encoder = Self {
            config,
            format,
            vorbis_source_channels,
            worker: Some(worker),
            packet_reader,
            output: VecDeque::new(),
            header_packets: Vec::new(),
            timing_headers: None,
            audio_packet_count: 0,
            relative_sample_cursor: 0,
            input_frame_count: 0,
            timeline_origin: None,
            input_metadata: Vec::new(),
            flushed: false,
        };
        encoder.collect_packets()?;
        if encoder.header_packets.len() != 3 {
            return Err(internal(
                "create_vorbis_encoder",
                "Vorbis encoder did not emit all three headers",
            ));
        }
        Ok(encoder)
    }

    fn collect_packets(&mut self) -> Result<()> {
        while let Some(packet) = self.packet_reader.read_packet().map_err(|source| {
            internal_source(
                "read_encoded_ogg_packet",
                "failed to recover a raw packet from encoded Ogg transport",
                source,
            )
        })? {
            if self.header_packets.len() < 3 {
                let index = self.header_packets.len();
                validate_header_packet(
                    &packet.data,
                    HEADER_TYPES[index],
                    "read_encoded_vorbis_header",
                )?;
                let data = Arc::<[u8]>::from(packet.data);
                self.header_packets.push(Arc::clone(&data));
                if self.header_packets.len() == 3 {
                    let packets: [Arc<[u8]>; 3] = self
                        .header_packets
                        .clone()
                        .try_into()
                        .expect("three encoded Vorbis headers were collected");
                    self.timing_headers = Some(VorbisHeaders::parse(&packets)?);
                }
                let timing = PacketTiming::new(self.config.timebase(), None, None, None)?;
                let mut output =
                    Packet::new(self.config.stream_id(), data, timing).with_keyframe(true);
                output.metadata_mut().insert(
                    "codec.header",
                    MetadataValue::Text(HEADER_NAMES[index].to_owned()),
                )?;
                self.output.push_back(output);
                continue;
            }

            let headers = self.timing_headers.as_ref().ok_or_else(|| {
                internal(
                    "read_encoded_vorbis_packet",
                    "Vorbis timing headers are unavailable",
                )
            })?;
            let nominal = if self.audio_packet_count == 0 {
                0
            } else {
                u64::try_from(
                    get_decoded_sample_count(&headers.ident, &headers.setup, &packet.data)
                        .map_err(|source| {
                            internal_source(
                                "read_encoded_vorbis_packet",
                                "encoded Vorbis packet could not be measured",
                                source,
                            )
                        })?,
                )
                .map_err(|_| {
                    internal(
                        "read_encoded_vorbis_packet",
                        "encoded Vorbis packet sample count overflowed",
                    )
                })?
            };
            let granule = packet.absgp_page();
            let duration = if packet.last_in_page()
                && granule != UNKNOWN_GRANULE_POSITION
                && granule >= self.relative_sample_cursor
            {
                granule - self.relative_sample_cursor
            } else {
                nominal
            };
            let origin = self.timeline_origin.ok_or_else(|| {
                internal(
                    "read_encoded_vorbis_packet",
                    "Vorbis audio was emitted before an input timeline origin was known",
                )
            })?;
            let relative = i64::try_from(self.relative_sample_cursor).map_err(|_| {
                internal(
                    "read_encoded_vorbis_packet",
                    "Vorbis packet timestamp overflowed",
                )
            })?;
            let timestamp = origin.checked_add(relative).ok_or_else(|| {
                internal(
                    "read_encoded_vorbis_packet",
                    "Vorbis packet timestamp overflowed",
                )
            })?;
            let timing = PacketTiming::new(
                self.config.timebase(),
                Some(timestamp),
                Some(timestamp),
                Some(duration),
            )?;
            let last_in_stream = packet.last_in_stream();
            let mut output = Packet::new(self.config.stream_id(), Arc::from(packet.data), timing);
            output
                .metadata_mut()
                .insert("codec.granule-position", MetadataValue::Unsigned(granule))?;
            output.metadata_mut().insert(
                "codec.end-of-stream",
                MetadataValue::Boolean(last_in_stream),
            )?;
            if duration > 0 {
                for (_, _, metadata) in self.input_metadata.iter().filter(|(start, end, _)| {
                    *start <= self.relative_sample_cursor && self.relative_sample_cursor < *end
                }) {
                    for (key, value) in metadata.iter() {
                        output.metadata_mut().insert(key, value.clone())?;
                    }
                }
            }
            self.relative_sample_cursor = self
                .relative_sample_cursor
                .checked_add(duration)
                .ok_or_else(|| {
                    internal(
                        "read_encoded_vorbis_packet",
                        "Vorbis encoded sample cursor overflowed",
                    )
                })?;
            self.audio_packet_count = self.audio_packet_count.checked_add(1).ok_or_else(|| {
                internal(
                    "read_encoded_vorbis_packet",
                    "Vorbis packet counter overflowed",
                )
            })?;
            self.output.push_back(output);
        }
        Ok(())
    }

    fn rebuild(&mut self) -> Result<()> {
        let (worker, packet_reader) = create_encoder_worker(&self.config, &self.format)?;
        self.worker = Some(worker);
        self.packet_reader = packet_reader;
        self.output.clear();
        self.header_packets.clear();
        self.timing_headers = None;
        self.audio_packet_count = 0;
        self.relative_sample_cursor = 0;
        self.input_frame_count = 0;
        self.timeline_origin = None;
        self.input_metadata.clear();
        self.flushed = false;
        self.collect_packets()
    }
}

impl Encoder for VorbisEncoder {
    fn config(&self) -> &EncoderConfig {
        &self.config
    }

    fn send(&mut self, input: EncodeInput, operation: &OperationContext) -> Result<()> {
        operation.check("send_vorbis_audio")?;
        if self.flushed {
            return Err(conflict(
                "send_vorbis_audio",
                "cannot send Vorbis audio after flush without reset",
            ));
        }
        let EncodeInput::Audio(block) = input else {
            return Err(invalid(
                "send_vorbis_audio",
                "Vorbis encoders accept only audio blocks",
            ));
        };
        if block.format() != &self.format {
            return Err(invalid(
                "send_vorbis_audio",
                "Vorbis audio block format does not match the encoder configuration",
            ));
        }
        let origin = *self
            .timeline_origin
            .get_or_insert(block.timestamp().sample());
        let expected = origin
            .checked_add(i64::try_from(self.input_frame_count).map_err(|_| {
                invalid("send_vorbis_audio", "Vorbis input sample cursor overflowed")
            })?)
            .ok_or_else(|| invalid("send_vorbis_audio", "Vorbis input sample cursor overflowed"))?;
        if block.timestamp().sample() != expected {
            return Err(conflict(
                "send_vorbis_audio",
                "Vorbis input blocks must be contiguous in sample time",
            ));
        }
        let samples = audio_block_to_f32(&block, &self.vorbis_source_channels, operation)?;
        let start = self.input_frame_count;
        self.input_frame_count = self
            .input_frame_count
            .checked_add(block.frame_count())
            .ok_or_else(|| invalid("send_vorbis_audio", "Vorbis input frame count overflowed"))?;
        if !block.metadata().is_empty() {
            self.input_metadata
                .push((start, self.input_frame_count, block.metadata().clone()));
        }
        self.worker
            .as_ref()
            .expect("active Vorbis encoder is present before flush")
            .encode(samples)
            .map_err(|source| {
                invalid_source(
                    "send_vorbis_audio",
                    "Vorbis encoder rejected the audio block",
                    source,
                )
            })?;
        operation.check("send_vorbis_audio")?;
        self.collect_packets()
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<EncodeOutput> {
        operation.check("receive_vorbis_packet")?;
        if let Some(packet) = self.output.pop_front() {
            return Ok(EncodeOutput::Packet(packet));
        }
        if self.flushed {
            Ok(EncodeOutput::EndOfStream)
        } else {
            Ok(EncodeOutput::NeedInput)
        }
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("flush_vorbis_encoder")?;
        if self.flushed {
            return Ok(());
        }
        self.timeline_origin.get_or_insert(0);
        let worker = self
            .worker
            .take()
            .expect("active Vorbis encoder is present before flush");
        worker.finish().map_err(|source| {
            internal_source(
                "flush_vorbis_encoder",
                "Vorbis encoder failed while draining",
                source,
            )
        })?;
        self.collect_packets()?;
        if self.relative_sample_cursor != self.input_frame_count {
            return Err(internal(
                "flush_vorbis_encoder",
                "Vorbis granule duration does not match submitted audio",
            ));
        }
        self.flushed = true;
        operation.check("flush_vorbis_encoder")
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("reset_vorbis_encoder")?;
        if let Some(mut worker) = self.worker.take() {
            worker.stop();
        }
        self.rebuild()
    }
}

fn create_encoder_worker(
    config: &EncoderConfig,
    format: &AudioFormat,
) -> Result<(VorbisEncoderWorker, OggPacketReader<SharedOggBuffer>)> {
    let sample_rate = NonZeroU32::new(format.sample_rate()).ok_or_else(|| {
        invalid(
            "create_vorbis_encoder",
            "Vorbis sample rate must be greater than zero",
        )
    })?;
    let channels = u8::try_from(format.channel_layout().len()).map_err(|_| {
        unsupported(
            "create_vorbis_encoder",
            "Vorbis channel count exceeds the supported range",
        )
    })?;
    let channels = NonZeroU8::new(channels).ok_or_else(|| {
        invalid(
            "create_vorbis_encoder",
            "Vorbis channel count must be greater than zero",
        )
    })?;
    let sink = SharedOggBuffer::default();
    let reader = OggPacketReader::new(sink.clone());
    let serial = i32::from_le_bytes(config.stream_id().value().to_le_bytes());
    let worker = VorbisEncoderWorker::start(sample_rate, channels, serial, sink)?;
    Ok((worker, reader))
}

fn validate_decoder_stream(config: &DecoderConfig) -> Result<()> {
    if config.stream().kind() != StreamKind::Audio {
        return Err(invalid(
            "create_vorbis_decoder",
            "Vorbis decoding requires an audio stream",
        ));
    }
    if config.stream().codec().as_str() != VORBIS_CODEC_ID {
        return Err(unsupported(
            "create_vorbis_decoder",
            "the requested codec is not Vorbis",
        ));
    }
    Ok(())
}

fn validate_requested_audio_format(config: &DecoderConfig, actual: &AudioFormat) -> Result<()> {
    if config
        .audio_format()
        .is_some_and(|requested| requested != actual)
    {
        return Err(invalid(
            "create_vorbis_decoder",
            "explicit Vorbis output format disagrees with the codec headers",
        ));
    }
    Ok(())
}

fn configured_header_packets(config: &DecoderConfig) -> Result<Option<[Arc<[u8]>; 3]>> {
    match config.stream().metadata().get("codec.configuration") {
        None => Ok(None),
        Some(MetadataValue::Bytes(bytes)) => split_xiph_laced_headers(bytes).map(Some),
        Some(_) => Err(corrupt(
            "create_vorbis_decoder",
            "Vorbis codec configuration metadata must contain bytes",
        )),
    }
}

fn split_xiph_laced_headers(configuration: &[u8]) -> Result<[Arc<[u8]>; 3]> {
    if configuration.first().copied() != Some(2) {
        return Err(corrupt(
            "parse_vorbis_configuration",
            "Vorbis Matroska configuration must contain exactly three Xiph-laced headers",
        ));
    }
    let mut position = 1_usize;
    let mut lengths = [0_usize; 2];
    for length in &mut lengths {
        loop {
            let value = *configuration.get(position).ok_or_else(|| {
                corrupt(
                    "parse_vorbis_configuration",
                    "Vorbis Xiph lacing ended inside a header length",
                )
            })?;
            position += 1;
            *length = length.checked_add(usize::from(value)).ok_or_else(|| {
                corrupt(
                    "parse_vorbis_configuration",
                    "Vorbis Xiph-laced header length overflowed",
                )
            })?;
            if value < 255 {
                break;
            }
        }
    }
    let first_end = position.checked_add(lengths[0]).ok_or_else(|| {
        corrupt(
            "parse_vorbis_configuration",
            "Vorbis header range overflowed",
        )
    })?;
    let second_end = first_end.checked_add(lengths[1]).ok_or_else(|| {
        corrupt(
            "parse_vorbis_configuration",
            "Vorbis header range overflowed",
        )
    })?;
    if position == first_end || first_end == second_end || second_end >= configuration.len() {
        return Err(corrupt(
            "parse_vorbis_configuration",
            "Vorbis Xiph-laced headers are empty or truncated",
        ));
    }
    Ok([
        Arc::from(&configuration[position..first_end]),
        Arc::from(&configuration[first_end..second_end]),
        Arc::from(&configuration[second_end..]),
    ])
}

fn validate_header_packet(packet: &[u8], expected: u8, operation: &'static str) -> Result<()> {
    if packet.len() < 7 || packet[0] != expected || &packet[1..7] != b"vorbis" {
        return Err(corrupt(
            operation,
            "Vorbis header packet type or capture pattern is invalid",
        ));
    }
    Ok(())
}

fn packet_sample_duration(packet: &Packet, sample_rate: u32) -> Result<Option<u64>> {
    let Some(duration) = packet.timing().duration() else {
        return Ok(None);
    };
    let target = Timebase::integer(sample_rate)?;
    duration
        .checked_rescale(target, TimeRounding::NearestTiesEven)
        .map(|value| Some(value.value()))
        .map_err(|source| {
            corrupt_source(
                "decode_vorbis_packet",
                "Vorbis packet duration cannot be mapped to the sample timeline",
                source,
            )
        })
}

fn packet_sample_timestamp(
    packet: &Packet,
    sample_rate: u32,
    inferred: Option<i64>,
) -> Result<SampleTime> {
    if let Some(inferred) = inferred {
        return SampleTime::new(inferred, sample_rate);
    }
    let Some(presentation) = packet.timing().presentation_time() else {
        return SampleTime::new(0, sample_rate);
    };
    let target = Timebase::integer(sample_rate)?;
    let converted = presentation
        .checked_rescale(target, TimeRounding::NearestTiesEven)
        .map_err(|source| {
            corrupt_source(
                "decode_vorbis_packet",
                "Vorbis presentation time cannot be mapped to the sample timeline",
                source,
            )
        })?;
    SampleTime::new(converted.value(), sample_rate)
}

fn vorbis_positions(channel_count: usize) -> Result<&'static [ChannelPosition]> {
    match channel_count {
        1 => Ok(&MONO_ORDER),
        2 => Ok(&STEREO_ORDER),
        3 => Ok(&THREE_CHANNEL_ORDER),
        4 => Ok(&QUAD_ORDER),
        5 => Ok(&FIVE_CHANNEL_ORDER),
        6 => Ok(&FIVE_ONE_ORDER),
        7 => Ok(&SIX_ONE_ORDER),
        8 => Ok(&SEVEN_ONE_ORDER),
        _ => Err(unsupported(
            "map_vorbis_channels",
            "Vorbis channel meaning is supported only for the standard one through eight channel mappings",
        )),
    }
}

fn canonical_layout(channel_count: usize) -> Result<ChannelLayout> {
    match channel_count {
        1 => Ok(ChannelLayout::mono()),
        2 => Ok(ChannelLayout::stereo()),
        3 => ChannelLayout::new([
            ChannelPosition::FrontLeft,
            ChannelPosition::FrontRight,
            ChannelPosition::FrontCenter,
        ]),
        4 => Ok(ChannelLayout::quad()),
        5 => ChannelLayout::new([
            ChannelPosition::FrontLeft,
            ChannelPosition::FrontRight,
            ChannelPosition::FrontCenter,
            ChannelPosition::BackLeft,
            ChannelPosition::BackRight,
        ]),
        6 => Ok(ChannelLayout::surround_5_1()),
        7 => ChannelLayout::new([
            ChannelPosition::FrontLeft,
            ChannelPosition::FrontRight,
            ChannelPosition::FrontCenter,
            ChannelPosition::LowFrequency,
            ChannelPosition::BackCenter,
            ChannelPosition::SideLeft,
            ChannelPosition::SideRight,
        ]),
        8 => Ok(ChannelLayout::surround_7_1()),
        _ => Err(unsupported(
            "map_vorbis_channels",
            "Vorbis channel meaning is supported only for the standard one through eight channel mappings",
        )),
    }
}

fn canonical_to_vorbis(layout: &ChannelLayout) -> Result<Vec<usize>> {
    let vorbis = vorbis_positions(layout.len())?;
    layout
        .positions()
        .iter()
        .map(|position| {
            vorbis
                .iter()
                .position(|candidate| candidate == position)
                .ok_or_else(|| {
                    unsupported(
                        "map_vorbis_channels",
                        "audio layout does not match a standard Vorbis channel mapping",
                    )
                })
        })
        .collect()
}

fn vorbis_source_channels(layout: &ChannelLayout) -> Result<Vec<usize>> {
    vorbis_positions(layout.len())?
        .iter()
        .map(|position| {
            layout
                .positions()
                .iter()
                .position(|candidate| candidate == position)
                .ok_or_else(|| {
                    unsupported(
                        "map_vorbis_channels",
                        "audio layout does not match a standard Vorbis channel mapping",
                    )
                })
        })
        .collect()
}

fn audio_block_to_f32(
    block: &AudioBlock,
    vorbis_source_channels: &[usize],
    operation: &OperationContext,
) -> Result<Vec<Vec<f32>>> {
    let format = block.format();
    let frames = usize::try_from(block.frame_count()).map_err(|_| {
        invalid(
            "send_vorbis_audio",
            "Vorbis input frame count cannot be represented on this platform",
        )
    })?;
    let bytes_per_sample = usize::from(format.sample_format().bytes_per_sample());
    let channel_count = format.channel_layout().len();
    let mut output = vorbis_source_channels
        .iter()
        .map(|_| Vec::with_capacity(frames))
        .collect::<Vec<_>>();
    for frame in 0..frames {
        if frame % 1_024 == 0 {
            operation.check("convert_vorbis_audio")?;
        }
        for (target, &source_channel) in vorbis_source_channels.iter().enumerate() {
            let (plane, offset) = if format.sample_format().is_planar() {
                (&block.planes()[source_channel], frame * bytes_per_sample)
            } else {
                (
                    &block.planes()[0],
                    (frame * channel_count + source_channel) * bytes_per_sample,
                )
            };
            let sample = decode_sample(
                &plane.bytes()[offset..offset + bytes_per_sample],
                format.sample_format(),
            );
            if !sample.is_finite() {
                return Err(invalid(
                    "send_vorbis_audio",
                    "Vorbis input samples must be finite",
                ));
            }
            output[target].push(sample);
        }
    }
    Ok(output)
}

fn decode_sample(bytes: &[u8], format: SampleFormat) -> f32 {
    match format {
        SampleFormat::U8 | SampleFormat::U8Planar => (f32::from(bytes[0]) - 128.0) / 128.0,
        SampleFormat::I16 | SampleFormat::I16Planar => {
            f32::from(i16::from_le_bytes(
                bytes.try_into().expect("validated i16 sample"),
            )) / 32_768.0
        }
        SampleFormat::I24 | SampleFormat::I24Planar => {
            let sign = if bytes[2] & 0x80 == 0 { 0 } else { 0xff };
            i32::from_le_bytes([bytes[0], bytes[1], bytes[2], sign]) as f32 / 8_388_608.0
        }
        SampleFormat::I32 | SampleFormat::I32Planar => {
            (f64::from(i32::from_le_bytes(
                bytes.try_into().expect("validated i32 sample"),
            )) / 2_147_483_648.0) as f32
        }
        SampleFormat::F32 | SampleFormat::F32Planar => {
            f32::from_le_bytes(bytes.try_into().expect("validated f32 sample"))
        }
        SampleFormat::F64 | SampleFormat::F64Planar => {
            f64::from_le_bytes(bytes.try_into().expect("validated f64 sample")) as f32
        }
        _ => f32::NAN,
    }
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-rs.vorbis", operation))
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-rs.vorbis", operation))
}

fn corrupt(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-rs.vorbis", operation))
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-rs.vorbis", operation))
}

fn internal(operation: &'static str, message: &'static str) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
        .with_context(ErrorContext::new("superi-codecs-rs.vorbis", operation))
}

fn invalid_source<E>(operation: &'static str, message: &'static str, source: E) -> Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    Error::with_source(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
        source,
    )
    .with_context(ErrorContext::new("superi-codecs-rs.vorbis", operation))
}

fn corrupt_source<E>(operation: &'static str, message: &'static str, source: E) -> Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    Error::with_source(
        ErrorCategory::CorruptData,
        Recoverability::Degraded,
        message,
        source,
    )
    .with_context(ErrorContext::new("superi-codecs-rs.vorbis", operation))
}

fn internal_source<E>(operation: &'static str, message: &'static str, source: E) -> Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    Error::with_source(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        message,
        source,
    )
    .with_context(ErrorContext::new("superi-codecs-rs.vorbis", operation))
}
