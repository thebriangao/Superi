//! Native Windows Media Foundation transform discovery and execution.
#![allow(unsafe_code)]

use std::collections::{BTreeSet, VecDeque};
use std::mem::ManuallyDrop;
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};

use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::{AlphaMode, ChannelLayout, PixelFormat, SampleFormat};
use superi_core::time::{Duration, RationalTime, SampleTime, TimeRounding, Timebase};
use superi_media_io::audio_io::{AudioBlock, AudioFormat, AudioPlane};
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, BackendDescriptor, BackendRegistration, BackendTier,
    MediaBackend,
};
use superi_media_io::decode::{
    CpuVideoBuffer, DecodeOutput, Decoder, DecoderConfig, VideoFormat, VideoFrame, VideoPlane,
};
use superi_media_io::demux::{
    BackendId, MediaMetadata, MediaSource, MetadataValue, Packet, PacketTiming, SourceProbe,
    SourceProbeResult, SourceRequest, StreamKind,
};
use superi_media_io::encode::{
    EncodeInput, EncodeOutput, Encoder, EncoderConfig, EncoderMediaFormat,
};
use superi_media_io::operation::OperationContext;
use windows::core::GUID;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED,
};

use super::{
    AacConfiguration, AnnexBConfiguration, MediaFoundationCodec, MediaFoundationOperation,
    ProResProfile,
};

const COMPONENT: &str = "superi-codecs-platform.media-foundation.windows";
const BACKEND_ID: &str = "windows-media-foundation";
const BACKEND_PRIORITY: u16 = 200;
const INPUT_STREAM_ID: u32 = 0;
const OUTPUT_STREAM_ID: u32 = 0;

/// Media Foundation backend whose capabilities reflect transforms installed on this host.
pub struct MediaFoundationBackend {
    descriptor: BackendDescriptor,
    supported: BTreeSet<(MediaFoundationOperation, MediaFoundationCodec)>,
}

impl MediaFoundationBackend {
    /// Discovers synchronous transforms and builds one primary registration when any are usable.
    pub fn registration() -> Result<Option<BackendRegistration>> {
        let supported = run_discovery()?;
        if supported.is_empty() {
            return Ok(None);
        }
        let capabilities =
            BackendCapabilities::new(supported.iter().map(|(operation, codec)| match operation {
                MediaFoundationOperation::Decode => BackendCapability::Decode(codec.codec_id()),
                MediaFoundationOperation::Encode => BackendCapability::Encode(codec.codec_id()),
            }));
        let backend = Arc::new(Self {
            descriptor: BackendDescriptor::new(
                BackendId::new(BACKEND_ID)?,
                "Windows Media Foundation",
            )?,
            supported,
        });
        BackendRegistration::new(
            backend,
            capabilities,
            BACKEND_PRIORITY,
            BackendTier::Primary,
        )
        .map(Some)
    }

    fn supports(&self, operation: MediaFoundationOperation, codec: MediaFoundationCodec) -> bool {
        self.supported.contains(&(operation, codec))
    }
}

impl MediaBackend for MediaFoundationBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn probe_source(
        &self,
        _probe: &SourceProbe<'_>,
        operation: &OperationContext,
    ) -> Result<SourceProbeResult> {
        operation.check("media_foundation_probe_source")?;
        Ok(SourceProbeResult::NoMatch)
    }

    fn open_source(
        &self,
        _request: &SourceRequest,
        operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>> {
        operation.check("media_foundation_open_source")?;
        Err(unsupported(
            "open_source",
            "Media Foundation owns codec transforms while container backends own sources",
        ))
    }

    fn create_decoder(
        &self,
        config: &DecoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Decoder>> {
        operation.check("media_foundation_create_decoder")?;
        let codec =
            MediaFoundationCodec::from_codec_id(config.stream().codec()).ok_or_else(|| {
                unsupported(
                    "create_decoder",
                    "codec is not routed through Media Foundation",
                )
            })?;
        if !self.supports(MediaFoundationOperation::Decode, codec) {
            return Err(unsupported(
                "create_decoder",
                "no discovered Media Foundation decoder supports this codec",
            ));
        }
        Ok(Box::new(MediaFoundationDecoder::spawn(
            config.clone(),
            codec,
            operation,
        )?))
    }

    fn create_encoder(
        &self,
        config: &EncoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>> {
        operation.check("media_foundation_create_encoder")?;
        let codec = MediaFoundationCodec::from_codec_id(config.codec()).ok_or_else(|| {
            unsupported(
                "create_encoder",
                "codec is not routed through Media Foundation",
            )
        })?;
        if !self.supports(MediaFoundationOperation::Encode, codec) {
            return Err(unsupported(
                "create_encoder",
                "no discovered Media Foundation encoder supports this codec",
            ));
        }
        Ok(Box::new(MediaFoundationEncoder::spawn(
            config.clone(),
            codec,
            operation,
        )?))
    }
}

fn run_discovery() -> Result<BTreeSet<(MediaFoundationOperation, MediaFoundationCodec)>> {
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("superi-mf-discovery".to_owned())
        .spawn(move || {
            let result = (|| {
                let _runtime = ComMfRuntime::enter("discover_transforms")?;
                let mut supported = BTreeSet::new();
                let mut codecs = vec![
                    MediaFoundationCodec::H264,
                    MediaFoundationCodec::Hevc,
                    MediaFoundationCodec::Aac,
                ];
                codecs.extend(
                    ProResProfile::ALL
                        .iter()
                        .copied()
                        .map(MediaFoundationCodec::ProRes),
                );
                for codec in codecs {
                    if codec == MediaFoundationCodec::ProRes(ProResProfile::FourFourFourFour) {
                        continue;
                    }
                    for operation in MediaFoundationOperation::ALL {
                        if enumerate_activations(codec, *operation)?.is_empty() {
                            continue;
                        }
                        supported.insert((*operation, codec));
                    }
                }
                Ok(supported)
            })();
            let _ = sender.send(result);
        })
        .map_err(|error| thread_error("discover_transforms", error))?;
    receiver
        .recv()
        .map_err(|_| worker_closed("discover_transforms"))?
}

fn enumerate_activations(
    codec: MediaFoundationCodec,
    operation: MediaFoundationOperation,
) -> Result<Vec<IMFActivate>> {
    let (category, input, output) = transform_signature(codec, operation);
    let mut raw: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0_u32;
    let flags = MFT_ENUM_FLAG_SYNCMFT
        | MFT_ENUM_FLAG_LOCALMFT
        | MFT_ENUM_FLAG_TRANSCODE_ONLY
        | MFT_ENUM_FLAG_SORTANDFILTER;
    unsafe {
        MFTEnumEx(
            category,
            flags,
            Some(&input),
            Some(&output),
            &mut raw,
            &mut count,
        )
    }
    .map_err(|error| windows_error("enumerate_transforms", error))?;
    let mut activations = Vec::with_capacity(count as usize);
    if !raw.is_null() {
        unsafe {
            let entries = std::slice::from_raw_parts_mut(raw, count as usize);
            for entry in entries {
                if let Some(activation) = entry.take() {
                    activations.push(activation);
                }
            }
            CoTaskMemFree(Some(raw.cast()));
        }
    }
    Ok(activations)
}

fn activate_transform(
    codec: MediaFoundationCodec,
    operation: MediaFoundationOperation,
) -> Result<IMFTransform> {
    let activation = enumerate_activations(codec, operation)?
        .into_iter()
        .next()
        .ok_or_else(|| {
            unsupported(
                "activate_transform",
                "the selected Media Foundation transform is no longer installed",
            )
        })?;
    unsafe { activation.ActivateObject::<IMFTransform>() }
        .map_err(|error| windows_error("activate_transform", error))
}

fn transform_signature(
    codec: MediaFoundationCodec,
    operation: MediaFoundationOperation,
) -> (GUID, MFT_REGISTER_TYPE_INFO, MFT_REGISTER_TYPE_INFO) {
    let compressed_major = match codec {
        MediaFoundationCodec::Aac => MFMediaType_Audio,
        _ => MFMediaType_Video,
    };
    let compressed_subtype = compressed_subtype(codec);
    let raw = match codec {
        MediaFoundationCodec::Aac => MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Audio,
            guidSubtype: MFAudioFormat_PCM,
        },
        MediaFoundationCodec::ProRes(_) => MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: MFVideoFormat_v210,
        },
        _ => MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: MFVideoFormat_NV12,
        },
    };
    let compressed = MFT_REGISTER_TYPE_INFO {
        guidMajorType: compressed_major,
        guidSubtype: compressed_subtype,
    };
    match operation {
        MediaFoundationOperation::Decode => (
            match codec {
                MediaFoundationCodec::Aac => MFT_CATEGORY_AUDIO_DECODER,
                _ => MFT_CATEGORY_VIDEO_DECODER,
            },
            compressed,
            raw,
        ),
        MediaFoundationOperation::Encode => (
            match codec {
                MediaFoundationCodec::Aac => MFT_CATEGORY_AUDIO_ENCODER,
                _ => MFT_CATEGORY_VIDEO_ENCODER,
            },
            raw,
            compressed,
        ),
    }
}

fn compressed_subtype(codec: MediaFoundationCodec) -> GUID {
    match codec {
        MediaFoundationCodec::H264 => MFVideoFormat_H264,
        MediaFoundationCodec::Hevc => MFVideoFormat_HEVC,
        MediaFoundationCodec::Aac => MFAudioFormat_AAC,
        MediaFoundationCodec::ProRes(profile) => fourcc_guid(profile.fourcc()),
    }
}

fn fourcc_guid(code: [u8; 4]) -> GUID {
    let value = u32::from_le_bytes(code);
    GUID::from_values(
        value,
        0,
        0x0010,
        [0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71],
    )
}

struct ComMfRuntime;

impl ComMfRuntime {
    fn enter(operation: &'static str) -> Result<Self> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|error| windows_error(operation, error))?;
        if let Err(error) = unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) } {
            unsafe { CoUninitialize() };
            return Err(windows_error(operation, error));
        }
        Ok(Self)
    }
}

impl Drop for ComMfRuntime {
    fn drop(&mut self) {
        let _ = unsafe { MFShutdown() };
        unsafe { CoUninitialize() };
    }
}

enum DecoderCommand {
    Send(Packet, OperationContext, mpsc::SyncSender<Result<()>>),
    Receive(OperationContext, mpsc::SyncSender<Result<DecodeOutput>>),
    Flush(OperationContext, mpsc::SyncSender<Result<()>>),
    Reset(OperationContext, mpsc::SyncSender<Result<()>>),
    Shutdown,
}

struct MediaFoundationDecoder {
    config: DecoderConfig,
    sender: mpsc::Sender<DecoderCommand>,
    worker: Option<JoinHandle<()>>,
}

impl MediaFoundationDecoder {
    fn spawn(
        config: DecoderConfig,
        codec: MediaFoundationCodec,
        operation: &OperationContext,
    ) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let worker_config = config.clone();
        let initial_operation = operation.clone();
        let worker = thread::Builder::new()
            .name("superi-mf-decoder".to_owned())
            .spawn(move || {
                let state = DecoderState::new(worker_config, codec, &initial_operation);
                match state {
                    Ok(mut state) => {
                        if ready_sender.send(Ok(())).is_err() {
                            return;
                        }
                        while let Ok(command) = receiver.recv() {
                            match command {
                                DecoderCommand::Send(packet, operation, reply) => {
                                    let _ = reply.send(state.send_packet(packet, &operation));
                                }
                                DecoderCommand::Receive(operation, reply) => {
                                    let _ = reply.send(state.receive(&operation));
                                }
                                DecoderCommand::Flush(operation, reply) => {
                                    let _ = reply.send(state.flush(&operation));
                                }
                                DecoderCommand::Reset(operation, reply) => {
                                    let _ = reply.send(state.reset(&operation));
                                }
                                DecoderCommand::Shutdown => break,
                            }
                        }
                    }
                    Err(error) => {
                        let _ = ready_sender.send(Err(error));
                    }
                }
            })
            .map_err(|error| thread_error("create_decoder", error))?;
        ready_receiver
            .recv()
            .map_err(|_| worker_closed("create_decoder"))??;
        Ok(Self {
            config,
            sender,
            worker: Some(worker),
        })
    }

    fn request<T>(
        &self,
        make: impl FnOnce(mpsc::SyncSender<Result<T>>) -> DecoderCommand,
        operation: &'static str,
    ) -> Result<T> {
        let (reply_sender, reply_receiver) = mpsc::sync_channel(1);
        self.sender
            .send(make(reply_sender))
            .map_err(|_| worker_closed(operation))?;
        reply_receiver
            .recv()
            .map_err(|_| worker_closed(operation))?
    }
}

impl Decoder for MediaFoundationDecoder {
    fn config(&self) -> &DecoderConfig {
        &self.config
    }

    fn send_packet(&mut self, packet: Packet, operation: &OperationContext) -> Result<()> {
        self.request(
            |reply| DecoderCommand::Send(packet, operation.clone(), reply),
            "send_packet",
        )
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<DecodeOutput> {
        self.request(
            |reply| DecoderCommand::Receive(operation.clone(), reply),
            "receive_decoded_output",
        )
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        self.request(
            |reply| DecoderCommand::Flush(operation.clone(), reply),
            "flush_decoder",
        )
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        self.request(
            |reply| DecoderCommand::Reset(operation.clone(), reply),
            "reset_decoder",
        )
    }
}

impl Drop for MediaFoundationDecoder {
    fn drop(&mut self) {
        let _ = self.sender.send(DecoderCommand::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

enum EncoderCommand {
    Send(EncodeInput, OperationContext, mpsc::SyncSender<Result<()>>),
    Receive(OperationContext, mpsc::SyncSender<Result<EncodeOutput>>),
    Flush(OperationContext, mpsc::SyncSender<Result<()>>),
    Reset(OperationContext, mpsc::SyncSender<Result<()>>),
    Shutdown,
}

struct MediaFoundationEncoder {
    config: EncoderConfig,
    sender: mpsc::Sender<EncoderCommand>,
    worker: Option<JoinHandle<()>>,
}

impl MediaFoundationEncoder {
    fn spawn(
        config: EncoderConfig,
        codec: MediaFoundationCodec,
        operation: &OperationContext,
    ) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let worker_config = config.clone();
        let initial_operation = operation.clone();
        let worker = thread::Builder::new()
            .name("superi-mf-encoder".to_owned())
            .spawn(move || {
                let state = EncoderState::new(worker_config, codec, &initial_operation);
                match state {
                    Ok(mut state) => {
                        if ready_sender.send(Ok(())).is_err() {
                            return;
                        }
                        while let Ok(command) = receiver.recv() {
                            match command {
                                EncoderCommand::Send(input, operation, reply) => {
                                    let _ = reply.send(state.send(input, &operation));
                                }
                                EncoderCommand::Receive(operation, reply) => {
                                    let _ = reply.send(state.receive(&operation));
                                }
                                EncoderCommand::Flush(operation, reply) => {
                                    let _ = reply.send(state.flush(&operation));
                                }
                                EncoderCommand::Reset(operation, reply) => {
                                    let _ = reply.send(state.reset(&operation));
                                }
                                EncoderCommand::Shutdown => break,
                            }
                        }
                    }
                    Err(error) => {
                        let _ = ready_sender.send(Err(error));
                    }
                }
            })
            .map_err(|error| thread_error("create_encoder", error))?;
        ready_receiver
            .recv()
            .map_err(|_| worker_closed("create_encoder"))??;
        Ok(Self {
            config,
            sender,
            worker: Some(worker),
        })
    }

    fn request<T>(
        &self,
        make: impl FnOnce(mpsc::SyncSender<Result<T>>) -> EncoderCommand,
        operation: &'static str,
    ) -> Result<T> {
        let (reply_sender, reply_receiver) = mpsc::sync_channel(1);
        self.sender
            .send(make(reply_sender))
            .map_err(|_| worker_closed(operation))?;
        reply_receiver
            .recv()
            .map_err(|_| worker_closed(operation))?
    }
}

impl Encoder for MediaFoundationEncoder {
    fn config(&self) -> &EncoderConfig {
        &self.config
    }

    fn send(&mut self, input: EncodeInput, operation: &OperationContext) -> Result<()> {
        self.request(
            |reply| EncoderCommand::Send(input, operation.clone(), reply),
            "send_encoder_input",
        )
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<EncodeOutput> {
        self.request(
            |reply| EncoderCommand::Receive(operation.clone(), reply),
            "receive_encoded_output",
        )
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        self.request(
            |reply| EncoderCommand::Flush(operation.clone(), reply),
            "flush_encoder",
        )
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        self.request(
            |reply| EncoderCommand::Reset(operation.clone(), reply),
            "reset_encoder",
        )
    }
}

impl Drop for MediaFoundationEncoder {
    fn drop(&mut self) {
        let _ = self.sender.send(EncoderCommand::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Clone)]
struct Provenance {
    hns: i64,
    timestamp: RationalTime,
    duration: Duration,
    metadata: MediaMetadata,
}

#[derive(Clone)]
enum DecodedFormat {
    Video(VideoFormat),
    Audio(AudioFormat),
}

struct DecoderState {
    config: DecoderConfig,
    codec: MediaFoundationCodec,
    transform: IMFTransform,
    decoded_format: DecodedFormat,
    output_info: MFT_OUTPUT_STREAM_INFO,
    annex_b: Option<AnnexBConfiguration>,
    provenance: VecDeque<Provenance>,
    ready: VecDeque<DecodeOutput>,
    flushing: bool,
    drained: bool,
    next_hns: i64,
    _runtime: ComMfRuntime,
}

impl DecoderState {
    fn new(
        config: DecoderConfig,
        codec: MediaFoundationCodec,
        operation: &OperationContext,
    ) -> Result<Self> {
        operation.check("initialize_media_foundation_decoder")?;
        validate_decoder_kind(&config, codec)?;
        let runtime = ComMfRuntime::enter("initialize_decoder_runtime")?;
        let transform = activate_transform(codec, MediaFoundationOperation::Decode)?;
        let annex_b = decoder_annex_configuration(&config, codec)?;
        let decoded_format = configure_decoder(&transform, &config, codec)?;
        let output_info = unsafe { transform.GetOutputStreamInfo(OUTPUT_STREAM_ID) }
            .map_err(|error| windows_error("get_decoder_output_stream_info", error))?;
        start_transform(&transform)?;
        Ok(Self {
            config,
            codec,
            transform,
            decoded_format,
            output_info,
            annex_b,
            provenance: VecDeque::new(),
            ready: VecDeque::new(),
            flushing: false,
            drained: false,
            next_hns: 0,
            _runtime: runtime,
        })
    }

    fn send_packet(&mut self, packet: Packet, operation: &OperationContext) -> Result<()> {
        operation.check("media_foundation_send_packet")?;
        if self.flushing {
            return Err(conflict(
                "send_packet",
                "decoder input is closed until reset after flush",
            ));
        }
        if packet.stream_id() != self.config.stream().id() {
            return Err(invalid(
                "send_packet",
                "packet stream does not match decoder configuration",
            ));
        }
        if packet.timing().timebase() != self.config.stream().timebase() {
            return Err(invalid(
                "send_packet",
                "packet timebase does not match decoder configuration",
            ));
        }
        let bytes = match &self.annex_b {
            Some(configuration) => {
                configuration.convert_sample(packet.data(), packet.is_keyframe())?
            }
            None => packet.data().to_vec(),
        };
        let timing = packet.timing();
        let timestamp = timing
            .presentation_time()
            .or_else(|| timing.decode_time())
            .unwrap_or_else(|| {
                RationalTime::new(
                    hns_to_time(self.next_hns, self.config.stream().timebase()),
                    self.config.stream().timebase(),
                )
            });
        let duration = timing.duration().unwrap_or_else(|| {
            Duration::new(1, self.config.stream().timebase())
                .expect("one-unit stream duration is valid")
        });
        let hns = time_to_hns(timestamp)?;
        let duration_hns = duration_to_hns(duration)?;
        let sample = sample_from_bytes(&bytes, hns, duration_hns)?;
        let provenance = Provenance {
            hns,
            timestamp,
            duration,
            metadata: packet.metadata().clone(),
        };
        loop {
            operation.check("media_foundation_process_decoder_input")?;
            match unsafe { self.transform.ProcessInput(INPUT_STREAM_ID, &sample, 0) } {
                Ok(()) => {
                    self.next_hns = hns.saturating_add(duration_hns.max(1));
                    self.provenance.push_back(provenance);
                    return Ok(());
                }
                Err(error) if error.code() == MF_E_NOTACCEPTING => {
                    match self.pull_output(operation)? {
                        PullResult::Output(output) => self.ready.push_back(output),
                        PullResult::NeedInput => {
                            return Err(windows_error("process_decoder_input", error))
                        }
                    }
                }
                Err(error) => return Err(transform_error("process_decoder_input", error, true)),
            }
        }
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<DecodeOutput> {
        operation.check("media_foundation_receive_decoder_output")?;
        if self.flushing && self.drained {
            return Ok(DecodeOutput::EndOfStream);
        }
        if let Some(output) = self.ready.pop_front() {
            return Ok(output);
        }
        match self.pull_output(operation)? {
            PullResult::Output(output) => Ok(output),
            PullResult::NeedInput if self.flushing => {
                self.drained = true;
                Ok(DecodeOutput::EndOfStream)
            }
            PullResult::NeedInput => Ok(DecodeOutput::NeedInput),
        }
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("media_foundation_flush_decoder")?;
        if !self.flushing {
            unsafe {
                self.transform
                    .ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0)
                    .map_err(|error| windows_error("notify_decoder_end_of_stream", error))?;
                self.transform
                    .ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0)
                    .map_err(|error| windows_error("drain_decoder", error))?;
            }
            self.flushing = true;
            self.drained = false;
        }
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("media_foundation_reset_decoder")?;
        unsafe {
            self.transform
                .ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0)
                .map_err(|error| windows_error("reset_decoder", error))?;
            self.transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
                .map_err(|error| windows_error("restart_decoder", error))?;
        }
        self.provenance.clear();
        self.ready.clear();
        self.flushing = false;
        self.drained = false;
        self.next_hns = 0;
        Ok(())
    }

    fn pull_output(&mut self, operation: &OperationContext) -> Result<PullResult<DecodeOutput>> {
        operation.check("media_foundation_process_decoder_output")?;
        loop {
            match process_output_sample(&self.transform, self.output_info, true) {
                Ok(Some(sample)) => {
                    let output = self.decode_sample(sample)?;
                    return Ok(PullResult::Output(output));
                }
                Ok(None) => return Ok(PullResult::NeedInput),
                Err(ProcessOutputError::StreamChange) => {
                    self.decoded_format =
                        renegotiate_decoder_output(&self.transform, &self.config, self.codec)?;
                    self.output_info =
                        unsafe { self.transform.GetOutputStreamInfo(OUTPUT_STREAM_ID) }
                            .map_err(|error| windows_error("refresh_decoder_output_info", error))?;
                }
                Err(ProcessOutputError::Failure(error)) => return Err(error),
            }
        }
    }

    fn decode_sample(&mut self, sample: IMFSample) -> Result<DecodeOutput> {
        let hns = unsafe { sample.GetSampleTime() }.unwrap_or(self.next_hns);
        let sample_duration = unsafe { sample.GetSampleDuration() }.ok();
        let provenance = take_provenance(&mut self.provenance, hns);
        let metadata = provenance
            .as_ref()
            .map(|value| value.metadata.clone())
            .unwrap_or_default();
        match self.decoded_format.clone() {
            DecodedFormat::Video(format) => {
                let bytes = sample_bytes(&sample)?;
                let buffer = video_buffer_from_bytes(format, &bytes)?;
                let timestamp = provenance
                    .as_ref()
                    .map(|value| value.timestamp)
                    .unwrap_or_else(|| {
                        RationalTime::new(
                            hns_to_time(hns, self.config.stream().timebase()),
                            self.config.stream().timebase(),
                        )
                    });
                let duration = provenance
                    .as_ref()
                    .map(|value| value.duration)
                    .or_else(|| {
                        sample_duration
                            .map(|value| hns_to_duration(value, self.config.stream().timebase()))
                    })
                    .unwrap_or_else(|| {
                        Duration::new(1, self.config.stream().timebase())
                            .expect("one-unit stream duration is valid")
                    });
                let frame = add_frame_metadata(
                    VideoFrame::new(format, timestamp, duration, Arc::new(buffer))?,
                    &metadata,
                )?;
                Ok(DecodeOutput::Frame(frame))
            }
            DecodedFormat::Audio(format) => {
                let bytes = sample_bytes(&sample)?;
                let bytes_per_frame = format
                    .channel_layout()
                    .len()
                    .checked_mul(usize::from(format.sample_format().bytes_per_sample()))
                    .ok_or_else(|| invalid("decode_audio_sample", "audio frame size overflowed"))?;
                if bytes_per_frame == 0 || bytes.len() % bytes_per_frame != 0 {
                    return Err(corrupt(
                        "decode_audio_sample",
                        "Media Foundation returned a partial PCM audio frame",
                    ));
                }
                let frame_count = u64::try_from(bytes.len() / bytes_per_frame)
                    .map_err(|_| invalid("decode_audio_sample", "audio frame count overflowed"))?;
                let timestamp = provenance
                    .as_ref()
                    .and_then(|value| {
                        value
                            .timestamp
                            .checked_rescale(
                                Timebase::integer(format.sample_rate()).ok()?,
                                TimeRounding::NearestTiesEven,
                            )
                            .ok()
                    })
                    .map(|value| value.value())
                    .unwrap_or_else(|| hns_to_samples(hns, format.sample_rate()));
                let block = AudioBlock::new(
                    format.clone(),
                    SampleTime::new(timestamp, format.sample_rate())?,
                    frame_count,
                    vec![AudioPlane::new(Arc::from(bytes))],
                )?;
                Ok(DecodeOutput::Audio(add_audio_metadata(block, &metadata)?))
            }
        }
    }
}

impl Drop for DecoderState {
    fn drop(&mut self) {
        let _ = unsafe {
            self.transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0)
        };
    }
}

struct EncoderState {
    config: EncoderConfig,
    codec: MediaFoundationCodec,
    transform: IMFTransform,
    output_info: MFT_OUTPUT_STREAM_INFO,
    provenance: VecDeque<Provenance>,
    ready: VecDeque<EncodeOutput>,
    sequence_header: Option<Arc<[u8]>>,
    sequence_header_sent: bool,
    flushing: bool,
    drained: bool,
    _runtime: ComMfRuntime,
}

impl EncoderState {
    fn new(
        config: EncoderConfig,
        codec: MediaFoundationCodec,
        operation: &OperationContext,
    ) -> Result<Self> {
        operation.check("initialize_media_foundation_encoder")?;
        validate_encoder_kind(&config, codec)?;
        let runtime = ComMfRuntime::enter("initialize_encoder_runtime")?;
        let transform = activate_transform(codec, MediaFoundationOperation::Encode)?;
        let sequence_header = configure_encoder(&transform, &config, codec)?;
        let output_info = unsafe { transform.GetOutputStreamInfo(OUTPUT_STREAM_ID) }
            .map_err(|error| windows_error("get_encoder_output_stream_info", error))?;
        start_transform(&transform)?;
        Ok(Self {
            config,
            codec,
            transform,
            output_info,
            provenance: VecDeque::new(),
            ready: VecDeque::new(),
            sequence_header,
            sequence_header_sent: false,
            flushing: false,
            drained: false,
            _runtime: runtime,
        })
    }

    fn send(&mut self, input: EncodeInput, operation: &OperationContext) -> Result<()> {
        operation.check("media_foundation_send_encoder_input")?;
        if self.flushing {
            return Err(conflict(
                "send_encoder_input",
                "encoder input is closed until reset after flush",
            ));
        }
        let (bytes, timestamp, duration, metadata) = encode_input_bytes(&self.config, input)?;
        let hns = time_to_hns(timestamp)?;
        let duration_hns = duration_to_hns(duration)?;
        let sample = sample_from_bytes(&bytes, hns, duration_hns)?;
        let provenance = Provenance {
            hns,
            timestamp,
            duration,
            metadata,
        };
        loop {
            operation.check("media_foundation_process_encoder_input")?;
            match unsafe { self.transform.ProcessInput(INPUT_STREAM_ID, &sample, 0) } {
                Ok(()) => {
                    self.provenance.push_back(provenance);
                    return Ok(());
                }
                Err(error) if error.code() == MF_E_NOTACCEPTING => {
                    match self.pull_output(operation)? {
                        PullResult::Output(output) => self.ready.push_back(output),
                        PullResult::NeedInput => {
                            return Err(windows_error("process_encoder_input", error))
                        }
                    }
                }
                Err(error) => return Err(transform_error("process_encoder_input", error, false)),
            }
        }
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<EncodeOutput> {
        operation.check("media_foundation_receive_encoder_output")?;
        if self.flushing && self.drained {
            return Ok(EncodeOutput::EndOfStream);
        }
        if let Some(output) = self.ready.pop_front() {
            return Ok(output);
        }
        match self.pull_output(operation)? {
            PullResult::Output(output) => Ok(output),
            PullResult::NeedInput if self.flushing => {
                self.drained = true;
                Ok(EncodeOutput::EndOfStream)
            }
            PullResult::NeedInput => Ok(EncodeOutput::NeedInput),
        }
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("media_foundation_flush_encoder")?;
        if !self.flushing {
            unsafe {
                self.transform
                    .ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0)
                    .map_err(|error| windows_error("notify_encoder_end_of_stream", error))?;
                self.transform
                    .ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0)
                    .map_err(|error| windows_error("drain_encoder", error))?;
            }
            self.flushing = true;
            self.drained = false;
        }
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("media_foundation_reset_encoder")?;
        unsafe {
            self.transform
                .ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0)
                .map_err(|error| windows_error("reset_encoder", error))?;
            self.transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
                .map_err(|error| windows_error("restart_encoder", error))?;
        }
        self.provenance.clear();
        self.ready.clear();
        self.sequence_header_sent = false;
        self.flushing = false;
        self.drained = false;
        Ok(())
    }

    fn pull_output(&mut self, operation: &OperationContext) -> Result<PullResult<EncodeOutput>> {
        operation.check("media_foundation_process_encoder_output")?;
        loop {
            match process_output_sample(&self.transform, self.output_info, false) {
                Ok(Some(sample)) => return self.encode_sample(sample).map(PullResult::Output),
                Ok(None) => return Ok(PullResult::NeedInput),
                Err(ProcessOutputError::StreamChange) => {
                    self.sequence_header =
                        renegotiate_encoder_output(&self.transform, &self.config, self.codec)?;
                    self.output_info =
                        unsafe { self.transform.GetOutputStreamInfo(OUTPUT_STREAM_ID) }
                            .map_err(|error| windows_error("refresh_encoder_output_info", error))?;
                }
                Err(ProcessOutputError::Failure(error)) => return Err(error),
            }
        }
    }

    fn encode_sample(&mut self, sample: IMFSample) -> Result<EncodeOutput> {
        let bytes = sample_bytes(&sample)?;
        let hns = unsafe { sample.GetSampleTime() }
            .unwrap_or_else(|_| self.provenance.front().map_or(0, |value| value.hns));
        let provenance = take_provenance(&mut self.provenance, hns);
        let timestamp = provenance
            .as_ref()
            .map(|value| value.timestamp)
            .unwrap_or_else(|| {
                RationalTime::new(
                    hns_to_time(hns, self.config.timebase()),
                    self.config.timebase(),
                )
            });
        let duration = provenance
            .as_ref()
            .map(|value| value.duration)
            .or_else(|| {
                unsafe { sample.GetSampleDuration() }
                    .ok()
                    .map(|value| hns_to_duration(value, self.config.timebase()))
            })
            .unwrap_or_else(|| {
                Duration::new(1, self.config.timebase()).expect("one-unit duration is valid")
            });
        let mut packet = Packet::new(
            self.config.stream_id(),
            Arc::from(bytes),
            PacketTiming::new(
                self.config.timebase(),
                Some(timestamp.value()),
                Some(timestamp.value()),
                Some(duration.value()),
            )?,
        );
        let keyframe = unsafe { sample.GetUINT32(&MFSampleExtension_CleanPoint) }.unwrap_or(0) != 0;
        packet = packet.with_keyframe(keyframe);
        if let Some(provenance) = provenance {
            packet = add_packet_metadata(packet, &provenance.metadata)?;
        }
        if !self.sequence_header_sent {
            if let Some(header) = self.sequence_header.clone() {
                packet =
                    packet.with_metadata("codec.configuration", MetadataValue::Bytes(header))?;
            }
            self.sequence_header_sent = true;
        }
        Ok(EncodeOutput::Packet(packet))
    }
}

impl Drop for EncoderState {
    fn drop(&mut self) {
        let _ = unsafe {
            self.transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0)
        };
    }
}

enum PullResult<T> {
    Output(T),
    NeedInput,
}

fn validate_decoder_kind(config: &DecoderConfig, codec: MediaFoundationCodec) -> Result<()> {
    let expected = match codec {
        MediaFoundationCodec::Aac => StreamKind::Audio,
        _ => StreamKind::Video,
    };
    if config.stream().kind() != expected {
        return Err(invalid(
            "create_decoder",
            "codec and decoder stream kinds do not match",
        ));
    }
    Ok(())
}

fn validate_encoder_kind(config: &EncoderConfig, codec: MediaFoundationCodec) -> Result<()> {
    let matches = matches!(
        (codec, config.media_format()),
        (MediaFoundationCodec::Aac, EncoderMediaFormat::Audio(_))
            | (
                MediaFoundationCodec::H264
                    | MediaFoundationCodec::Hevc
                    | MediaFoundationCodec::ProRes(_),
                EncoderMediaFormat::Video(_)
            )
    );
    if !matches {
        return Err(invalid(
            "create_encoder",
            "codec and encoder media format kinds do not match",
        ));
    }
    Ok(())
}

fn decoder_annex_configuration(
    config: &DecoderConfig,
    codec: MediaFoundationCodec,
) -> Result<Option<AnnexBConfiguration>> {
    if !matches!(
        codec,
        MediaFoundationCodec::H264 | MediaFoundationCodec::Hevc
    ) {
        return Ok(None);
    }
    match config.stream().metadata().get("codec.configuration") {
        Some(MetadataValue::Bytes(bytes)) => AnnexBConfiguration::parse(codec, bytes).map(Some),
        Some(_) => Err(invalid(
            "create_decoder",
            "codec.configuration metadata must contain bytes",
        )),
        None => Ok(None),
    }
}

fn configure_decoder(
    transform: &IMFTransform,
    config: &DecoderConfig,
    codec: MediaFoundationCodec,
) -> Result<DecodedFormat> {
    match codec {
        MediaFoundationCodec::Aac => configure_aac_decoder(transform, config),
        _ => configure_video_decoder(transform, config, codec),
    }
}

fn configure_video_decoder(
    transform: &IMFTransform,
    config: &DecoderConfig,
    codec: MediaFoundationCodec,
) -> Result<DecodedFormat> {
    let (width, height) = video_dimensions(config.stream().metadata())?;
    let input = unsafe { MFCreateMediaType() }
        .map_err(|error| windows_error("create_video_decoder_input_type", error))?;
    set_guid(
        &input,
        &MF_MT_MAJOR_TYPE,
        &MFMediaType_Video,
        "set_video_input_major",
    )?;
    set_guid(
        &input,
        &MF_MT_SUBTYPE,
        &compressed_subtype(codec),
        "set_video_input_subtype",
    )?;
    set_size(
        &input,
        &MF_MT_FRAME_SIZE,
        width,
        height,
        "set_video_input_size",
    )?;
    unsafe { transform.SetInputType(INPUT_STREAM_ID, &input, 0) }
        .map_err(|error| transform_error("set_video_decoder_input_type", error, true))?;
    renegotiate_video_decoder_output(transform, config, codec, width, height)
}

fn renegotiate_video_decoder_output(
    transform: &IMFTransform,
    config: &DecoderConfig,
    codec: MediaFoundationCodec,
    fallback_width: u32,
    fallback_height: u32,
) -> Result<DecodedFormat> {
    let preferences = video_decoder_output_preferences(config, codec)?;
    for (subtype, pixel_format) in preferences {
        let mut index = 0_u32;
        loop {
            let media_type =
                match unsafe { transform.GetOutputAvailableType(OUTPUT_STREAM_ID, index) } {
                    Ok(media_type) => media_type,
                    Err(_) => break,
                };
            index = index.saturating_add(1);
            if unsafe { media_type.GetGUID(&MF_MT_SUBTYPE) }.ok() != Some(subtype) {
                continue;
            }
            if unsafe { transform.SetOutputType(OUTPUT_STREAM_ID, &media_type, 0) }.is_err() {
                continue;
            }
            let (width, height) = get_size(&media_type, &MF_MT_FRAME_SIZE)
                .unwrap_or((fallback_width, fallback_height));
            let format = VideoFormat::new(
                width,
                height,
                pixel_format,
                color_space_from_media_type(&media_type),
                AlphaMode::Opaque,
            )?;
            return Ok(DecodedFormat::Video(format));
        }
    }
    Err(unsupported(
        "set_video_decoder_output_type",
        "Media Foundation decoder exposes no lossless supported output representation",
    ))
}

fn video_decoder_output_preferences(
    config: &DecoderConfig,
    codec: MediaFoundationCodec,
) -> Result<Vec<(GUID, PixelFormat)>> {
    match codec {
        MediaFoundationCodec::H264 => {
            if let Some(MetadataValue::Bytes(configuration)) =
                config.stream().metadata().get("codec.configuration")
            {
                if configuration
                    .get(1)
                    .is_some_and(|profile| matches!(profile, 110 | 122 | 244))
                {
                    return Err(unsupported(
                        "create_video_decoder",
                        "10-bit H.264 cannot be represented by the supported Media Foundation output",
                    ));
                }
            }
            Ok(vec![(MFVideoFormat_NV12, PixelFormat::Nv12)])
        }
        MediaFoundationCodec::Hevc => {
            let main10 = match config.stream().metadata().get("codec.configuration") {
                Some(MetadataValue::Bytes(configuration)) if configuration.len() > 18 => {
                    configuration[1] & 0x1f == 2
                        || configuration[17] & 0x07 != 0
                        || configuration[18] & 0x07 != 0
                }
                _ => matches!(
                    config.stream().metadata().get("codec.rfc6381"),
                    Some(MetadataValue::Text(value))
                        if value.starts_with("hvc1.2.") || value.starts_with("hev1.2.")
                ),
            };
            if main10 {
                Ok(vec![(MFVideoFormat_P010, PixelFormat::P010)])
            } else {
                Ok(vec![(MFVideoFormat_NV12, PixelFormat::Nv12)])
            }
        }
        MediaFoundationCodec::ProRes(ProResProfile::FourFourFourFour) => Err(unsupported(
            "create_video_decoder",
            "the public frame contract cannot represent ProRes 4444 without losing alpha",
        )),
        MediaFoundationCodec::ProRes(_) => Ok(vec![(MFVideoFormat_v210, PixelFormat::Yuv422p10)]),
        MediaFoundationCodec::Aac => Err(invalid(
            "create_video_decoder",
            "AAC cannot use a video output type",
        )),
    }
}

fn configure_aac_decoder(
    transform: &IMFTransform,
    config: &DecoderConfig,
) -> Result<DecodedFormat> {
    let bytes = match config.stream().metadata().get("codec.configuration") {
        Some(MetadataValue::Bytes(bytes)) => bytes.as_ref(),
        Some(_) => {
            return Err(invalid(
                "create_aac_decoder",
                "codec.configuration metadata must contain bytes",
            ))
        }
        None => {
            return Err(unsupported(
                "create_aac_decoder",
                "AAC decoder requires AudioSpecificConfig metadata",
            ))
        }
    };
    let configuration = AacConfiguration::parse(bytes)?;
    let requested = if let Some(format) = config.audio_format() {
        if format.sample_rate() != configuration.sample_rate()
            || format.channel_layout().len() != usize::from(configuration.channel_count())
            || format.sample_format() != SampleFormat::I16
        {
            return Err(unsupported(
                "create_aac_decoder",
                "AAC output must be packed I16 at the configured rate and channel count",
            ));
        }
        format.clone()
    } else {
        AudioFormat::new(
            configuration.sample_rate(),
            SampleFormat::I16,
            channel_layout(configuration.channel_count())?,
        )?
    };
    let input = audio_media_type(
        MFAudioFormat_AAC,
        configuration.sample_rate(),
        u32::from(configuration.channel_count()),
        0,
    )?;
    unsafe {
        input
            .SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)
            .map_err(|error| windows_error("set_aac_payload_type", error))?;
        input
            .SetUINT32(&MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, 0xfe)
            .map_err(|error| windows_error("set_aac_profile", error))?;
        input
            .SetBlob(
                &MF_MT_USER_DATA,
                &configuration.media_foundation_user_data(),
            )
            .map_err(|error| windows_error("set_aac_user_data", error))?;
        transform
            .SetInputType(INPUT_STREAM_ID, &input, 0)
            .map_err(|error| transform_error("set_aac_decoder_input_type", error, true))?;
    }
    let output = audio_media_type(
        MFAudioFormat_PCM,
        requested.sample_rate(),
        u32::try_from(requested.channel_layout().len())
            .map_err(|_| invalid("create_aac_decoder", "channel count overflowed"))?,
        16,
    )?;
    unsafe { transform.SetOutputType(OUTPUT_STREAM_ID, &output, 0) }
        .map_err(|error| transform_error("set_aac_decoder_output_type", error, true))?;
    Ok(DecodedFormat::Audio(requested))
}

fn renegotiate_decoder_output(
    transform: &IMFTransform,
    config: &DecoderConfig,
    codec: MediaFoundationCodec,
) -> Result<DecodedFormat> {
    match codec {
        MediaFoundationCodec::Aac => configure_aac_decoder(transform, config),
        _ => {
            let (width, height) = video_dimensions(config.stream().metadata())?;
            renegotiate_video_decoder_output(transform, config, codec, width, height)
        }
    }
}

fn configure_encoder(
    transform: &IMFTransform,
    config: &EncoderConfig,
    codec: MediaFoundationCodec,
) -> Result<Option<Arc<[u8]>>> {
    match config.media_format() {
        EncoderMediaFormat::Video(format) => {
            configure_video_encoder(transform, config, codec, *format)
        }
        EncoderMediaFormat::Audio(format) => configure_aac_encoder(transform, format),
        _ => Err(unsupported(
            "create_encoder",
            "encoder media format is not supported by Media Foundation",
        )),
    }
}

fn configure_video_encoder(
    transform: &IMFTransform,
    config: &EncoderConfig,
    codec: MediaFoundationCodec,
    format: VideoFormat,
) -> Result<Option<Arc<[u8]>>> {
    let input_subtype = video_input_subtype(format.pixel_format())?;
    let output = unsafe { MFCreateMediaType() }
        .map_err(|error| windows_error("create_video_encoder_output_type", error))?;
    set_guid(
        &output,
        &MF_MT_MAJOR_TYPE,
        &MFMediaType_Video,
        "set_video_output_major",
    )?;
    set_guid(
        &output,
        &MF_MT_SUBTYPE,
        &compressed_subtype(codec),
        "set_video_output_subtype",
    )?;
    set_size(
        &output,
        &MF_MT_FRAME_SIZE,
        format.width(),
        format.height(),
        "set_video_output_size",
    )?;
    set_ratio(
        &output,
        &MF_MT_FRAME_RATE,
        config.timebase().numerator(),
        config.timebase().denominator(),
        "set_video_output_rate",
    )?;
    set_video_color(&output, format.color_space())?;
    let bitrate = default_video_bitrate(format, config.timebase());
    unsafe {
        output
            .SetUINT32(&MF_MT_AVG_BITRATE, bitrate)
            .map_err(|error| windows_error("set_video_output_bitrate", error))?;
        output
            .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            .map_err(|error| windows_error("set_video_output_interlace", error))?;
        transform
            .SetOutputType(OUTPUT_STREAM_ID, &output, 0)
            .map_err(|error| transform_error("set_video_encoder_output_type", error, false))?;
    }
    let input = unsafe { MFCreateMediaType() }
        .map_err(|error| windows_error("create_video_encoder_input_type", error))?;
    set_guid(
        &input,
        &MF_MT_MAJOR_TYPE,
        &MFMediaType_Video,
        "set_video_input_major",
    )?;
    set_guid(
        &input,
        &MF_MT_SUBTYPE,
        &input_subtype,
        "set_video_input_subtype",
    )?;
    set_size(
        &input,
        &MF_MT_FRAME_SIZE,
        format.width(),
        format.height(),
        "set_video_input_size",
    )?;
    set_ratio(
        &input,
        &MF_MT_FRAME_RATE,
        config.timebase().numerator(),
        config.timebase().denominator(),
        "set_video_input_rate",
    )?;
    set_video_color(&input, format.color_space())?;
    unsafe {
        if format.pixel_format() == PixelFormat::Yuv422p10 {
            input
                .SetUINT32(&MF_MT_DEFAULT_STRIDE, v210_stride(format.width())?)
                .map_err(|error| windows_error("set_video_input_stride", error))?;
        }
        input
            .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            .map_err(|error| windows_error("set_video_input_interlace", error))?;
        transform
            .SetInputType(INPUT_STREAM_ID, &input, 0)
            .map_err(|error| transform_error("set_video_encoder_input_type", error, false))?;
    }
    output_sequence_header(&output)
}

fn configure_aac_encoder(
    transform: &IMFTransform,
    format: &AudioFormat,
) -> Result<Option<Arc<[u8]>>> {
    if format.sample_format() != SampleFormat::I16
        || !matches!(format.sample_rate(), 44_100 | 48_000)
        || !matches!(format.channel_layout().len(), 1 | 2 | 6)
    {
        return Err(unsupported(
            "create_aac_encoder",
            "Media Foundation AAC encode supports packed I16 at 44.1 or 48 kHz with 1, 2, or 6 channels",
        ));
    }
    let channels = u32::try_from(format.channel_layout().len())
        .map_err(|_| invalid("create_aac_encoder", "channel count overflowed"))?;
    let output = audio_media_type(MFAudioFormat_AAC, format.sample_rate(), channels, 0)?;
    let average_bytes = if channels == 6 { 72_000 } else { 20_000 };
    unsafe {
        output
            .SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, average_bytes)
            .map_err(|error| windows_error("set_aac_output_bitrate", error))?;
        output
            .SetUINT32(&MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, 0x29)
            .map_err(|error| windows_error("set_aac_output_profile", error))?;
        transform
            .SetOutputType(OUTPUT_STREAM_ID, &output, 0)
            .map_err(|error| transform_error("set_aac_encoder_output_type", error, false))?;
    }
    let input = audio_media_type(MFAudioFormat_PCM, format.sample_rate(), channels, 16)?;
    unsafe { transform.SetInputType(INPUT_STREAM_ID, &input, 0) }
        .map_err(|error| transform_error("set_aac_encoder_input_type", error, false))?;
    let frequency_index = match format.sample_rate() {
        44_100 => 4_u16,
        48_000 => 3_u16,
        _ => unreachable!("validated AAC encoder sample rate"),
    };
    let audio_specific_config = (2_u16 << 11) | (frequency_index << 7) | ((channels as u16) << 3);
    Ok(Some(Arc::from(audio_specific_config.to_be_bytes())))
}

fn renegotiate_encoder_output(
    transform: &IMFTransform,
    config: &EncoderConfig,
    codec: MediaFoundationCodec,
) -> Result<Option<Arc<[u8]>>> {
    let mut index = 0_u32;
    while let Ok(media_type) = unsafe { transform.GetOutputAvailableType(OUTPUT_STREAM_ID, index) }
    {
        index = index.saturating_add(1);
        if unsafe { media_type.GetGUID(&MF_MT_SUBTYPE) }.ok() != Some(compressed_subtype(codec)) {
            continue;
        }
        if unsafe { transform.SetOutputType(OUTPUT_STREAM_ID, &media_type, 0) }.is_ok() {
            return output_sequence_header(&media_type);
        }
    }
    configure_encoder(transform, config, codec)
}

fn start_transform(transform: &IMFTransform) -> Result<()> {
    unsafe {
        transform
            .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
            .map_err(|error| windows_error("begin_transform_streaming", error))?;
        transform
            .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
            .map_err(|error| windows_error("start_transform_stream", error))?;
    }
    Ok(())
}

fn audio_media_type(
    subtype: GUID,
    sample_rate: u32,
    channels: u32,
    bits_per_sample: u32,
) -> Result<IMFMediaType> {
    let media_type = unsafe { MFCreateMediaType() }
        .map_err(|error| windows_error("create_audio_media_type", error))?;
    set_guid(
        &media_type,
        &MF_MT_MAJOR_TYPE,
        &MFMediaType_Audio,
        "set_audio_major",
    )?;
    set_guid(&media_type, &MF_MT_SUBTYPE, &subtype, "set_audio_subtype")?;
    unsafe {
        media_type
            .SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, sample_rate)
            .map_err(|error| windows_error("set_audio_sample_rate", error))?;
        media_type
            .SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels)
            .map_err(|error| windows_error("set_audio_channels", error))?;
        if bits_per_sample != 0 {
            let block_alignment = channels.checked_mul(bits_per_sample / 8).ok_or_else(|| {
                invalid(
                    "create_audio_media_type",
                    "audio block alignment overflowed",
                )
            })?;
            let bytes_per_second = sample_rate
                .checked_mul(block_alignment)
                .ok_or_else(|| invalid("create_audio_media_type", "audio byte rate overflowed"))?;
            media_type
                .SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, bits_per_sample)
                .map_err(|error| windows_error("set_audio_bits", error))?;
            media_type
                .SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, block_alignment)
                .map_err(|error| windows_error("set_audio_alignment", error))?;
            media_type
                .SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, bytes_per_second)
                .map_err(|error| windows_error("set_audio_byte_rate", error))?;
        }
    }
    Ok(media_type)
}

fn video_input_subtype(format: PixelFormat) -> Result<GUID> {
    match format {
        PixelFormat::Nv12 => Ok(MFVideoFormat_NV12),
        PixelFormat::P010 => Ok(MFVideoFormat_P010),
        PixelFormat::Bgra8Unorm => Ok(MFVideoFormat_ARGB32),
        PixelFormat::Yuv422p10 => Ok(MFVideoFormat_v210),
        _ => Err(unsupported(
            "create_video_encoder",
            "Media Foundation encoder accepts NV12, P010, BGRA8, or planar 10-bit 4:2:2 CPU frames",
        )),
    }
}

fn default_video_bitrate(format: VideoFormat, timebase: Timebase) -> u32 {
    let pixels = u64::from(format.width()) * u64::from(format.height());
    let frames = u64::from(timebase.numerator()) / u64::from(timebase.denominator()).max(1);
    pixels
        .saturating_mul(frames.max(1))
        .saturating_div(8)
        .clamp(128_000, 100_000_000) as u32
}

fn v210_stride(width: u32) -> Result<u32> {
    width
        .div_ceil(48)
        .checked_mul(128)
        .ok_or_else(|| invalid("create_video_encoder", "v210 row size overflowed"))
}

fn video_dimensions(metadata: &MediaMetadata) -> Result<(u32, u32)> {
    let value = |key: &str| match metadata.get(key) {
        Some(MetadataValue::Unsigned(value)) => u32::try_from(*value).ok(),
        _ => None,
    };
    let width = value("video.width").ok_or_else(|| {
        invalid(
            "create_video_decoder",
            "video.width metadata must contain a positive unsigned value",
        )
    })?;
    let height = value("video.height").ok_or_else(|| {
        invalid(
            "create_video_decoder",
            "video.height metadata must contain a positive unsigned value",
        )
    })?;
    if width == 0 || height == 0 {
        return Err(invalid(
            "create_video_decoder",
            "video dimensions must be greater than zero",
        ));
    }
    Ok((width, height))
}

fn channel_layout(channels: u8) -> Result<ChannelLayout> {
    match channels {
        1 => Ok(ChannelLayout::mono()),
        2 => Ok(ChannelLayout::stereo()),
        6 => Ok(ChannelLayout::surround_5_1()),
        _ => Err(unsupported(
            "create_audio_format",
            "Media Foundation channel layout is not represented by the stable audio contract",
        )),
    }
}

enum ProcessOutputError {
    StreamChange,
    Failure(Error),
}

fn process_output_sample(
    transform: &IMFTransform,
    info: MFT_OUTPUT_STREAM_INFO,
    decoding: bool,
) -> std::result::Result<Option<IMFSample>, ProcessOutputError> {
    let provides_samples = info.dwFlags & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32) != 0;
    let sample = if provides_samples {
        None
    } else {
        let sample = unsafe { MFCreateSample() }.map_err(|error| {
            ProcessOutputError::Failure(windows_error("create_output_sample", error))
        })?;
        let size = info.cbSize.max(1);
        let buffer = if info.cbAlignment == 0 {
            unsafe { MFCreateMemoryBuffer(size) }
        } else {
            unsafe { MFCreateAlignedMemoryBuffer(size, info.cbAlignment) }
        }
        .map_err(|error| {
            ProcessOutputError::Failure(windows_error("create_output_buffer", error))
        })?;
        unsafe { sample.AddBuffer(&buffer) }.map_err(|error| {
            ProcessOutputError::Failure(windows_error("attach_output_buffer", error))
        })?;
        Some(sample)
    };
    let mut output = MFT_OUTPUT_DATA_BUFFER {
        dwStreamID: OUTPUT_STREAM_ID,
        pSample: ManuallyDrop::new(sample),
        dwStatus: 0,
        pEvents: ManuallyDrop::new(None),
    };
    let mut status = 0_u32;
    let result =
        unsafe { transform.ProcessOutput(0, std::slice::from_mut(&mut output), &mut status) };
    let sample = unsafe { ManuallyDrop::take(&mut output.pSample) };
    let events = unsafe { ManuallyDrop::take(&mut output.pEvents) };
    drop(events);
    match result {
        Ok(()) => Ok(sample),
        Err(error) if error.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => Ok(None),
        Err(error) if error.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
            Err(ProcessOutputError::StreamChange)
        }
        Err(error) => Err(ProcessOutputError::Failure(transform_error(
            "process_transform_output",
            error,
            decoding,
        ))),
    }
}

fn sample_from_bytes(bytes: &[u8], hns: i64, duration_hns: i64) -> Result<IMFSample> {
    let length = u32::try_from(bytes.len()).map_err(|_| {
        invalid(
            "create_input_sample",
            "sample byte length exceeds Media Foundation",
        )
    })?;
    let sample =
        unsafe { MFCreateSample() }.map_err(|error| windows_error("create_input_sample", error))?;
    let buffer = unsafe { MFCreateMemoryBuffer(length.max(1)) }
        .map_err(|error| windows_error("create_input_buffer", error))?;
    if !bytes.is_empty() {
        let mut pointer = std::ptr::null_mut();
        unsafe { buffer.Lock(&mut pointer, None, None) }
            .map_err(|error| windows_error("lock_input_buffer", error))?;
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer, bytes.len()) };
        let unlock = unsafe { buffer.Unlock() };
        unlock.map_err(|error| windows_error("unlock_input_buffer", error))?;
    }
    unsafe {
        buffer
            .SetCurrentLength(length)
            .map_err(|error| windows_error("set_input_buffer_length", error))?;
        sample
            .AddBuffer(&buffer)
            .map_err(|error| windows_error("attach_input_buffer", error))?;
        sample
            .SetSampleTime(hns)
            .map_err(|error| windows_error("set_input_sample_time", error))?;
        sample
            .SetSampleDuration(duration_hns.max(1))
            .map_err(|error| windows_error("set_input_sample_duration", error))?;
    }
    Ok(sample)
}

fn sample_bytes(sample: &IMFSample) -> Result<Vec<u8>> {
    let buffer = unsafe { sample.ConvertToContiguousBuffer() }
        .map_err(|error| windows_error("make_output_contiguous", error))?;
    let mut pointer = std::ptr::null_mut();
    let mut current = 0_u32;
    unsafe { buffer.Lock(&mut pointer, None, Some(&mut current)) }
        .map_err(|error| windows_error("lock_output_buffer", error))?;
    let bytes = unsafe { std::slice::from_raw_parts(pointer, current as usize) }.to_vec();
    unsafe { buffer.Unlock() }.map_err(|error| windows_error("unlock_output_buffer", error))?;
    Ok(bytes)
}

fn video_buffer_from_bytes(format: VideoFormat, bytes: &[u8]) -> Result<CpuVideoBuffer> {
    let width = usize::try_from(format.width())
        .map_err(|_| invalid("decode_video_sample", "video width overflowed"))?;
    let height = usize::try_from(format.height())
        .map_err(|_| invalid("decode_video_sample", "video height overflowed"))?;
    match format.pixel_format() {
        PixelFormat::Nv12 | PixelFormat::P010 => {
            let bytes_per_component = if format.pixel_format() == PixelFormat::P010 {
                2
            } else {
                1
            };
            let minimum_stride = width
                .checked_mul(bytes_per_component)
                .ok_or_else(|| invalid("decode_video_sample", "video row byte size overflowed"))?;
            let chroma_rows = height.div_ceil(2);
            let total_rows = height
                .checked_add(chroma_rows)
                .ok_or_else(|| invalid("decode_video_sample", "video row count overflowed"))?;
            let inferred_stride = bytes.len().checked_div(total_rows).unwrap_or(0);
            let stride = if inferred_stride >= minimum_stride
                && inferred_stride.saturating_mul(total_rows) == bytes.len()
            {
                inferred_stride
            } else {
                minimum_stride
            };
            let luma_length = stride
                .checked_mul(height)
                .ok_or_else(|| invalid("decode_video_sample", "luma size overflowed"))?;
            let chroma_length = stride
                .checked_mul(chroma_rows)
                .ok_or_else(|| invalid("decode_video_sample", "chroma size overflowed"))?;
            let expected = luma_length
                .checked_add(chroma_length)
                .ok_or_else(|| invalid("decode_video_sample", "video size overflowed"))?;
            if bytes.len() < expected {
                return Err(corrupt(
                    "decode_video_sample",
                    "Media Foundation returned a truncated video frame",
                ));
            }
            let luma = VideoPlane::new(
                Arc::from(bytes[..luma_length].to_vec()),
                stride,
                format.height(),
            )?;
            let chroma = VideoPlane::new(
                Arc::from(bytes[luma_length..expected].to_vec()),
                stride,
                u32::try_from(chroma_rows)
                    .map_err(|_| invalid("decode_video_sample", "chroma rows overflowed"))?,
            )?;
            CpuVideoBuffer::new(
                format.width(),
                format.height(),
                format.pixel_format(),
                vec![luma, chroma],
            )
        }
        PixelFormat::Yuv422p10 => unpack_v210(format, bytes),
        _ => Err(unsupported(
            "decode_video_sample",
            "negotiated Media Foundation pixel format is not CPU-addressable",
        )),
    }
}

fn encode_input_bytes(
    config: &EncoderConfig,
    input: EncodeInput,
) -> Result<(Vec<u8>, RationalTime, Duration, MediaMetadata)> {
    match (config.media_format(), input) {
        (EncoderMediaFormat::Video(expected), EncodeInput::Video(frame)) => {
            if frame.format() != *expected {
                return Err(invalid(
                    "send_encoder_input",
                    "video frame format does not match encoder configuration",
                ));
            }
            if frame.timestamp().timebase() != config.timebase()
                || frame.duration().timebase() != config.timebase()
            {
                return Err(invalid(
                    "send_encoder_input",
                    "video frame timing does not match encoder timebase",
                ));
            }
            let cpu = frame
                .buffer()
                .as_any()
                .downcast_ref::<CpuVideoBuffer>()
                .ok_or_else(|| {
                    unsupported(
                        "send_encoder_input",
                        "synchronous Media Foundation transforms require CPU video frames",
                    )
                })?;
            let bytes = flatten_video_buffer(cpu, *expected)?;
            Ok((
                bytes,
                frame.timestamp(),
                frame.duration(),
                frame.metadata().clone(),
            ))
        }
        (EncoderMediaFormat::Audio(expected), EncodeInput::Audio(block)) => {
            if block.format() != expected {
                return Err(invalid(
                    "send_encoder_input",
                    "audio block format does not match encoder configuration",
                ));
            }
            if block.planes().len() != 1 {
                return Err(unsupported(
                    "send_encoder_input",
                    "Media Foundation AAC encoder requires packed audio",
                ));
            }
            Ok((
                block.planes()[0].bytes().to_vec(),
                block.timestamp().rational_time(),
                block.duration(),
                block.metadata().clone(),
            ))
        }
        _ => Err(invalid(
            "send_encoder_input",
            "encoder input kind does not match encoder configuration",
        )),
    }
}

fn flatten_video_buffer(buffer: &CpuVideoBuffer, format: VideoFormat) -> Result<Vec<u8>> {
    let width = usize::try_from(format.width())
        .map_err(|_| invalid("send_video_frame", "video width overflowed"))?;
    let mut output = Vec::new();
    match format.pixel_format() {
        PixelFormat::Nv12 | PixelFormat::P010 => {
            let bytes_per_component = if format.pixel_format() == PixelFormat::P010 {
                2
            } else {
                1
            };
            let row_bytes = width
                .checked_mul(bytes_per_component)
                .ok_or_else(|| invalid("send_video_frame", "video row byte size overflowed"))?;
            for plane in buffer.planes() {
                append_plane_rows(&mut output, plane, row_bytes)?;
            }
        }
        PixelFormat::Bgra8Unorm => {
            let row_bytes = width
                .checked_mul(4)
                .ok_or_else(|| invalid("send_video_frame", "video row byte size overflowed"))?;
            append_plane_rows(&mut output, &buffer.planes()[0], row_bytes)?;
        }
        PixelFormat::Yuv422p10 => return pack_v210(buffer, format),
        _ => {
            return Err(unsupported(
                "send_video_frame",
                "Media Foundation encoder input pixel format is unsupported",
            ))
        }
    }
    Ok(output)
}

fn unpack_v210(format: VideoFormat, bytes: &[u8]) -> Result<CpuVideoBuffer> {
    let width = usize::try_from(format.width())
        .map_err(|_| invalid("decode_v210", "video width overflowed"))?;
    let height = usize::try_from(format.height())
        .map_err(|_| invalid("decode_v210", "video height overflowed"))?;
    let minimum_stride = width
        .div_ceil(6)
        .checked_mul(16)
        .ok_or_else(|| invalid("decode_v210", "v210 row size overflowed"))?;
    let inferred_stride = bytes.len().checked_div(height).unwrap_or(0);
    let stride = if inferred_stride >= minimum_stride
        && inferred_stride.saturating_mul(height) == bytes.len()
    {
        inferred_stride
    } else {
        minimum_stride
    };
    let required = stride
        .checked_mul(height)
        .ok_or_else(|| invalid("decode_v210", "v210 frame size overflowed"))?;
    if bytes.len() < required {
        return Err(corrupt(
            "decode_v210",
            "Media Foundation returned a truncated v210 frame",
        ));
    }
    let chroma_width = width.div_ceil(2);
    let y_length = width
        .checked_mul(height)
        .and_then(|value| value.checked_mul(2))
        .ok_or_else(|| invalid("decode_v210", "luma plane size overflowed"))?;
    let chroma_length = chroma_width
        .checked_mul(height)
        .and_then(|value| value.checked_mul(2))
        .ok_or_else(|| invalid("decode_v210", "chroma plane size overflowed"))?;
    let mut y = vec![0_u8; y_length];
    let mut u = vec![0_u8; chroma_length];
    let mut v = vec![0_u8; chroma_length];

    for row in 0..height {
        let row_start = row
            .checked_mul(stride)
            .ok_or_else(|| invalid("decode_v210", "v210 row offset overflowed"))?;
        for group in 0..width.div_ceil(6) {
            let start = row_start
                .checked_add(group.saturating_mul(16))
                .ok_or_else(|| invalid("decode_v210", "v210 group offset overflowed"))?;
            let words = [
                read_v210_word(bytes, start)?,
                read_v210_word(bytes, start + 4)?,
                read_v210_word(bytes, start + 8)?,
                read_v210_word(bytes, start + 12)?,
            ];
            let y_values = [
                (words[0] >> 10) & 0x03ff,
                words[1] & 0x03ff,
                (words[1] >> 20) & 0x03ff,
                (words[2] >> 10) & 0x03ff,
                words[3] & 0x03ff,
                (words[3] >> 20) & 0x03ff,
            ];
            let u_values = [
                words[0] & 0x03ff,
                (words[1] >> 10) & 0x03ff,
                (words[2] >> 20) & 0x03ff,
            ];
            let v_values = [
                (words[0] >> 20) & 0x03ff,
                words[2] & 0x03ff,
                (words[3] >> 10) & 0x03ff,
            ];
            for (offset, value) in y_values.into_iter().enumerate() {
                let column = group * 6 + offset;
                if column < width {
                    write_u16_plane(&mut y, row * width + column, value as u16)?;
                }
            }
            for offset in 0..3 {
                let column = group * 3 + offset;
                if column < chroma_width {
                    let index = row * chroma_width + column;
                    write_u16_plane(&mut u, index, u_values[offset] as u16)?;
                    write_u16_plane(&mut v, index, v_values[offset] as u16)?;
                }
            }
        }
    }

    CpuVideoBuffer::new(
        format.width(),
        format.height(),
        PixelFormat::Yuv422p10,
        vec![
            VideoPlane::new(Arc::from(y), width * 2, format.height())?,
            VideoPlane::new(Arc::from(u), chroma_width * 2, format.height())?,
            VideoPlane::new(Arc::from(v), chroma_width * 2, format.height())?,
        ],
    )
}

fn pack_v210(buffer: &CpuVideoBuffer, format: VideoFormat) -> Result<Vec<u8>> {
    let width = usize::try_from(format.width())
        .map_err(|_| invalid("encode_v210", "video width overflowed"))?;
    let height = usize::try_from(format.height())
        .map_err(|_| invalid("encode_v210", "video height overflowed"))?;
    let stride = usize::try_from(v210_stride(format.width())?)
        .map_err(|_| invalid("encode_v210", "v210 row size overflowed"))?;
    let frame_length = stride
        .checked_mul(height)
        .ok_or_else(|| invalid("encode_v210", "v210 frame size overflowed"))?;
    let mut output = vec![0_u8; frame_length];
    let [y, u, v] = buffer.planes() else {
        return Err(invalid(
            "encode_v210",
            "planar 10-bit 4:2:2 input requires Y, U, and V planes",
        ));
    };
    for row in 0..height {
        for group in 0..width.div_ceil(6) {
            let base_pixel = group * 6;
            let base_chroma = group * 3;
            let y_values = [
                read_u16_plane(y, row, base_pixel, width, 64)?,
                read_u16_plane(y, row, base_pixel + 1, width, 64)?,
                read_u16_plane(y, row, base_pixel + 2, width, 64)?,
                read_u16_plane(y, row, base_pixel + 3, width, 64)?,
                read_u16_plane(y, row, base_pixel + 4, width, 64)?,
                read_u16_plane(y, row, base_pixel + 5, width, 64)?,
            ];
            let chroma_width = width.div_ceil(2);
            let u_values = [
                read_u16_plane(u, row, base_chroma, chroma_width, 512)?,
                read_u16_plane(u, row, base_chroma + 1, chroma_width, 512)?,
                read_u16_plane(u, row, base_chroma + 2, chroma_width, 512)?,
            ];
            let v_values = [
                read_u16_plane(v, row, base_chroma, chroma_width, 512)?,
                read_u16_plane(v, row, base_chroma + 1, chroma_width, 512)?,
                read_u16_plane(v, row, base_chroma + 2, chroma_width, 512)?,
            ];
            let words = [
                u32::from(u_values[0])
                    | (u32::from(y_values[0]) << 10)
                    | (u32::from(v_values[0]) << 20),
                u32::from(y_values[1])
                    | (u32::from(u_values[1]) << 10)
                    | (u32::from(y_values[2]) << 20),
                u32::from(v_values[1])
                    | (u32::from(y_values[3]) << 10)
                    | (u32::from(u_values[2]) << 20),
                u32::from(y_values[4])
                    | (u32::from(v_values[2]) << 10)
                    | (u32::from(y_values[5]) << 20),
            ];
            let start = row * stride + group * 16;
            for (index, word) in words.into_iter().enumerate() {
                output[start + index * 4..start + index * 4 + 4]
                    .copy_from_slice(&word.to_le_bytes());
            }
        }
    }
    Ok(output)
}

fn read_v210_word(bytes: &[u8], start: usize) -> Result<u32> {
    let value = bytes
        .get(start..start + 4)
        .ok_or_else(|| corrupt("decode_v210", "v210 word is truncated"))?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn write_u16_plane(bytes: &mut [u8], index: usize, value: u16) -> Result<()> {
    let start = index
        .checked_mul(2)
        .ok_or_else(|| invalid("decode_v210", "planar sample offset overflowed"))?;
    let target = bytes
        .get_mut(start..start + 2)
        .ok_or_else(|| corrupt("decode_v210", "planar sample range is invalid"))?;
    target.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn read_u16_plane(
    plane: &VideoPlane,
    row: usize,
    column: usize,
    width: usize,
    padding: u16,
) -> Result<u16> {
    if column >= width {
        return Ok(padding);
    }
    let start = row
        .checked_mul(plane.stride())
        .and_then(|value| value.checked_add(column.saturating_mul(2)))
        .ok_or_else(|| invalid("encode_v210", "planar sample offset overflowed"))?;
    let bytes = plane
        .bytes()
        .get(start..start + 2)
        .ok_or_else(|| invalid("encode_v210", "planar sample range is invalid"))?;
    let value = u16::from_le_bytes([bytes[0], bytes[1]]);
    if value > 0x03ff {
        return Err(invalid(
            "encode_v210",
            "10-bit planar input contains a sample above 1023",
        ));
    }
    Ok(value)
}

fn append_plane_rows(output: &mut Vec<u8>, plane: &VideoPlane, row_bytes: usize) -> Result<()> {
    if plane.stride() < row_bytes {
        return Err(invalid(
            "send_video_frame",
            "video plane stride is smaller than the encoded row width",
        ));
    }
    for row in 0..usize::try_from(plane.row_count())
        .map_err(|_| invalid("send_video_frame", "video row count overflowed"))?
    {
        let start = row
            .checked_mul(plane.stride())
            .ok_or_else(|| invalid("send_video_frame", "video plane offset overflowed"))?;
        let end = start
            .checked_add(row_bytes)
            .ok_or_else(|| invalid("send_video_frame", "video plane range overflowed"))?;
        output.extend_from_slice(&plane.bytes()[start..end]);
    }
    Ok(())
}

fn take_provenance(values: &mut VecDeque<Provenance>, hns: i64) -> Option<Provenance> {
    let position = values
        .iter()
        .position(|value| value.hns == hns)
        .unwrap_or(0);
    values.remove(position)
}

fn add_frame_metadata(mut frame: VideoFrame, metadata: &MediaMetadata) -> Result<VideoFrame> {
    for (key, value) in metadata.iter() {
        frame = frame.with_metadata(key, value.clone())?;
    }
    Ok(frame)
}

fn add_audio_metadata(mut block: AudioBlock, metadata: &MediaMetadata) -> Result<AudioBlock> {
    for (key, value) in metadata.iter() {
        block = block.with_metadata(key, value.clone())?;
    }
    Ok(block)
}

fn add_packet_metadata(mut packet: Packet, metadata: &MediaMetadata) -> Result<Packet> {
    for (key, value) in metadata.iter() {
        packet = packet.with_metadata(key, value.clone())?;
    }
    Ok(packet)
}

fn color_space_from_media_type(media_type: &IMFMediaType) -> ColorSpace {
    let value = |key: &GUID| unsafe { media_type.GetUINT32(key) }.ok();
    let primaries = match value(&MF_MT_VIDEO_PRIMARIES) {
        Some(value) if value == MFVideoPrimaries_BT709.0 as u32 => ColorPrimaries::Bt709,
        Some(value) if value == MFVideoPrimaries_BT2020.0 as u32 => ColorPrimaries::Bt2020,
        Some(value) if value == MFVideoPrimaries_DCI_P3.0 as u32 => ColorPrimaries::DisplayP3,
        Some(value) if value == MFVideoPrimaries_ACES.0 as u32 => ColorPrimaries::AcesAp0,
        _ => ColorPrimaries::Unspecified,
    };
    let transfer = match value(&MF_MT_TRANSFER_FUNCTION) {
        Some(value) if value == MFVideoTransFunc_sRGB.0 as u32 => TransferFunction::Srgb,
        Some(value) if value == MFVideoTransFunc_709.0 as u32 => TransferFunction::Bt709,
        Some(value) if value == MFVideoTransFunc_2020.0 as u32 => TransferFunction::Bt2020TenBit,
        Some(value) if value == MFVideoTransFunc_2084.0 as u32 => TransferFunction::Pq,
        Some(value) if value == MFVideoTransFunc_HLG.0 as u32 => TransferFunction::Hlg,
        Some(value) if value == MFVideoTransFunc_22.0 as u32 => TransferFunction::Gamma22,
        Some(value) if value == MFVideoTransFunc_26.0 as u32 => TransferFunction::Gamma24,
        _ => TransferFunction::Unspecified,
    };
    let matrix = match value(&MF_MT_YUV_MATRIX) {
        Some(value) if value == MFVideoTransferMatrix_BT601.0 as u32 => MatrixCoefficients::Bt601,
        Some(value) if value == MFVideoTransferMatrix_BT709.0 as u32 => MatrixCoefficients::Bt709,
        Some(value)
            if value == MFVideoTransferMatrix_BT2020_10.0 as u32
                || value == MFVideoTransferMatrix_BT2020_12.0 as u32 =>
        {
            MatrixCoefficients::Bt2020NonConstant
        }
        _ => MatrixCoefficients::Unspecified,
    };
    let range = match value(&MF_MT_VIDEO_NOMINAL_RANGE) {
        Some(value) if value == MFNominalRange_0_255.0 as u32 => ColorRange::Full,
        Some(value) if value == MFNominalRange_16_235.0 as u32 => ColorRange::Limited,
        _ => ColorRange::Unspecified,
    };
    ColorSpace::new(primaries, transfer, matrix, range)
}

fn set_video_color(media_type: &IMFMediaType, color: ColorSpace) -> Result<()> {
    let primaries = match color.primaries() {
        ColorPrimaries::Bt709 => Some(MFVideoPrimaries_BT709.0 as u32),
        ColorPrimaries::Bt2020 => Some(MFVideoPrimaries_BT2020.0 as u32),
        ColorPrimaries::DisplayP3 => Some(MFVideoPrimaries_DCI_P3.0 as u32),
        ColorPrimaries::AcesAp0 => Some(MFVideoPrimaries_ACES.0 as u32),
        _ => None,
    };
    let transfer = match color.transfer() {
        TransferFunction::Srgb => Some(MFVideoTransFunc_sRGB.0 as u32),
        TransferFunction::Bt709 => Some(MFVideoTransFunc_709.0 as u32),
        TransferFunction::Bt2020TenBit | TransferFunction::Bt2020TwelveBit => {
            Some(MFVideoTransFunc_2020.0 as u32)
        }
        TransferFunction::Pq => Some(MFVideoTransFunc_2084.0 as u32),
        TransferFunction::Hlg => Some(MFVideoTransFunc_HLG.0 as u32),
        TransferFunction::Gamma22 => Some(MFVideoTransFunc_22.0 as u32),
        TransferFunction::Gamma24 => Some(MFVideoTransFunc_26.0 as u32),
        _ => None,
    };
    let matrix = match color.matrix() {
        MatrixCoefficients::Bt601 => Some(MFVideoTransferMatrix_BT601.0 as u32),
        MatrixCoefficients::Bt709 => Some(MFVideoTransferMatrix_BT709.0 as u32),
        MatrixCoefficients::Bt2020NonConstant | MatrixCoefficients::Bt2020Constant => {
            Some(MFVideoTransferMatrix_BT2020_10.0 as u32)
        }
        _ => None,
    };
    let range = match color.range() {
        ColorRange::Full => Some(MFNominalRange_0_255.0 as u32),
        ColorRange::Limited => Some(MFNominalRange_16_235.0 as u32),
        _ => None,
    };
    for (key, value, operation) in [
        (&MF_MT_VIDEO_PRIMARIES, primaries, "set_video_primaries"),
        (&MF_MT_TRANSFER_FUNCTION, transfer, "set_video_transfer"),
        (&MF_MT_YUV_MATRIX, matrix, "set_video_matrix"),
        (&MF_MT_VIDEO_NOMINAL_RANGE, range, "set_video_range"),
    ] {
        if let Some(value) = value {
            unsafe { media_type.SetUINT32(key, value) }
                .map_err(|error| windows_error(operation, error))?;
        }
    }
    Ok(())
}

fn output_sequence_header(media_type: &IMFMediaType) -> Result<Option<Arc<[u8]>>> {
    let size = match unsafe { media_type.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER) } {
        Ok(size) if size != 0 => size,
        _ => return Ok(None),
    };
    let mut bytes = vec![0_u8; size as usize];
    unsafe { media_type.GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut bytes, None) }
        .map_err(|error| windows_error("get_encoder_sequence_header", error))?;
    Ok(Some(Arc::from(bytes)))
}

fn set_guid(
    media_type: &IMFMediaType,
    key: &GUID,
    value: &GUID,
    operation: &'static str,
) -> Result<()> {
    unsafe { media_type.SetGUID(key, value) }.map_err(|error| windows_error(operation, error))
}

fn set_size(
    media_type: &IMFMediaType,
    key: &GUID,
    width: u32,
    height: u32,
    operation: &'static str,
) -> Result<()> {
    unsafe { media_type.SetUINT64(key, (u64::from(width) << 32) | u64::from(height)) }
        .map_err(|error| windows_error(operation, error))
}

fn set_ratio(
    media_type: &IMFMediaType,
    key: &GUID,
    numerator: u32,
    denominator: u32,
    operation: &'static str,
) -> Result<()> {
    unsafe { media_type.SetUINT64(key, (u64::from(numerator) << 32) | u64::from(denominator)) }
        .map_err(|error| windows_error(operation, error))
}

fn get_size(media_type: &IMFMediaType, key: &GUID) -> Option<(u32, u32)> {
    let value = unsafe { media_type.GetUINT64(key) }.ok()?;
    Some(((value >> 32) as u32, value as u32))
}

fn hns_timebase() -> Timebase {
    Timebase::integer(10_000_000).expect("Media Foundation HNS timebase is valid")
}

fn time_to_hns(value: RationalTime) -> Result<i64> {
    Ok(value
        .checked_rescale(hns_timebase(), TimeRounding::NearestTiesEven)?
        .value())
}

fn duration_to_hns(value: Duration) -> Result<i64> {
    let converted = value.checked_rescale(hns_timebase(), TimeRounding::NearestTiesEven)?;
    i64::try_from(converted.value()).map_err(|_| {
        invalid(
            "convert_duration",
            "duration exceeds Media Foundation range",
        )
    })
}

fn hns_to_time(hns: i64, timebase: Timebase) -> i64 {
    RationalTime::new(hns, hns_timebase())
        .checked_rescale(timebase, TimeRounding::NearestTiesEven)
        .map_or(0, |value| value.value())
}

fn hns_to_duration(hns: i64, timebase: Timebase) -> Duration {
    let hns = u64::try_from(hns.max(0)).unwrap_or(0);
    Duration::new(hns, hns_timebase())
        .and_then(|value| value.checked_rescale(timebase, TimeRounding::NearestTiesEven))
        .unwrap_or_else(|_| Duration::zero(timebase))
}

fn hns_to_samples(hns: i64, sample_rate: u32) -> i64 {
    hns_to_time(
        hns,
        Timebase::integer(sample_rate).expect("validated audio sample rate"),
    )
}

fn windows_error(operation: &'static str, error: windows::core::Error) -> Error {
    Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        "Windows Media Foundation operation failed",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("hresult", format!("0x{:08x}", error.code().0 as u32))
            .with_field("windows_message", error.message()),
    )
}

fn transform_error(operation: &'static str, error: windows::core::Error, decoding: bool) -> Error {
    let code = error.code();
    let (category, recoverability, message) = if code == MF_E_INVALIDMEDIATYPE
        || code == MF_E_TOPO_CODEC_NOT_FOUND
        || code == MF_E_TRANSFORM_TYPE_NOT_SET
        || code == MF_E_TRANSFORM_NOT_POSSIBLE_FOR_CURRENT_MEDIATYPE_COMBINATION
    {
        (
            ErrorCategory::Unsupported,
            Recoverability::Degraded,
            "Media Foundation transform does not support the requested media variation",
        )
    } else if decoding {
        (
            ErrorCategory::CorruptData,
            Recoverability::Degraded,
            "Media Foundation rejected compressed media data",
        )
    } else {
        (
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            "Media Foundation transform failed",
        )
    };
    Error::new(category, recoverability, message).with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("hresult", format!("0x{:08x}", code.0 as u32))
            .with_field("windows_message", error.message()),
    )
}

fn thread_error(operation: &'static str, error: std::io::Error) -> Error {
    Error::with_source(
        ErrorCategory::ResourceExhausted,
        Recoverability::Retryable,
        "failed to create a Media Foundation worker thread",
        error,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn worker_closed(operation: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        "Media Foundation worker stopped unexpectedly",
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
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
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn corrupt(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v210_roundtrip_preserves_odd_width_planar_ten_bit_samples() {
        let format = VideoFormat::new(
            7,
            2,
            PixelFormat::Yuv422p10,
            ColorSpace::BT709,
            AlphaMode::Opaque,
        )
        .unwrap();
        let plane = |width: usize, seed: u16| {
            let mut bytes = Vec::new();
            for row in 0..2_u16 {
                for column in 0..width as u16 {
                    bytes.extend_from_slice(&(seed + row * 16 + column).to_le_bytes());
                }
            }
            VideoPlane::new(Arc::from(bytes), width * 2, 2).unwrap()
        };
        let source = CpuVideoBuffer::new(
            7,
            2,
            PixelFormat::Yuv422p10,
            vec![plane(7, 64), plane(4, 256), plane(4, 512)],
        )
        .unwrap();

        let packed = pack_v210(&source, format).unwrap();
        let decoded = unpack_v210(format, &packed).unwrap();

        assert_eq!(decoded.planes(), source.planes());
    }
}
