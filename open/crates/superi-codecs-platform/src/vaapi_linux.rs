//! Linux-only VA-API implementation.

use std::any::Any;
use std::collections::{BTreeSet, VecDeque};
use std::env;
use std::fmt;
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use cros_codecs::backend::vaapi::decoder::VaapiBackend as CrosVaapiDecoderBackend;
use cros_codecs::decoder::stateless::{
    h264::H264, h265::H265, DecodeError, DynStatelessVideoDecoder, StatelessDecoder,
    StatelessVideoDecoder,
};
use cros_codecs::decoder::{BlockingMode, DecodedHandle, DecoderEvent};
use cros_codecs::libva::{Display, VAEntrypoint, VAProfile};
use cros_codecs::video_frame::frame_pool::{FramePool, PooledVideoFrame};
use cros_codecs::video_frame::gbm_video_frame::{GbmDevice, GbmUsage};
use cros_codecs::video_frame::generic_dma_video_frame::GenericDmaVideoFrame;
use cros_codecs::video_frame::VideoFrame as CrosVideoFrame;
use cros_codecs::{DecodedFormat, Fourcc};
use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::time::{Duration, RationalTime};
use superi_media_io::backend::{BackendDescriptor, BackendRegistration, BackendTier, MediaBackend};
use superi_media_io::decode::{
    CpuVideoBuffer, DecodeOutput, Decoder, DecoderConfig, VideoFormat, VideoFrame, VideoFrameBuffer,
};
use superi_media_io::demux::{
    BackendId, MediaMetadata, MediaSource, MetadataValue, Packet, PacketTiming, SourceProbe,
    SourceProbeResult, SourceRequest, StreamKind,
};
use superi_media_io::encode::{
    EncodeInput, EncodeOutput, Encoder, EncoderConfig, EncoderMediaFormat,
};
use superi_media_io::operation::OperationContext;

use super::{
    capability_set, categorized, conflict, corrupt, normalize_avc_access_unit,
    normalize_hevc_access_unit, unsupported, validate_opaque_alpha, CodecLifecycle,
    DriverCapabilities, H264Profile, TimingLedger, H264_CODEC_ID, HEVC_CODEC_ID,
};

const BACKEND_ID: &str = "linux-vaapi";
const BACKEND_PRIORITY: u16 = 200;
const RENDER_NODE_ENV: &str = "SUPERI_VAAPI_RENDER_NODE";
const MAX_PENDING_PACKETS: usize = 64;
const CORRUPT_NATIVE_PREFIX: &str = "corrupt compressed input: ";

type DmaFrame = PooledVideoFrame<GenericDmaVideoFrame>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VideoCodec {
    H264,
    Hevc,
}

impl VideoCodec {
    fn from_id(id: &str) -> Option<Self> {
        match id {
            H264_CODEC_ID => Some(Self::H264),
            HEVC_CODEC_ID => Some(Self::Hevc),
            _ => None,
        }
    }

    const fn id(self) -> &'static str {
        match self {
            Self::H264 => H264_CODEC_ID,
            Self::Hevc => HEVC_CODEC_ID,
        }
    }
}

#[derive(Clone, Debug)]
struct DriverProbe {
    render_node: PathBuf,
    vendor: String,
    capabilities: DriverCapabilities,
}

/// Linux VA-API backend bound to one probed DRM render node.
pub struct VaapiBackend {
    descriptor: BackendDescriptor,
    probe: DriverProbe,
}

impl VaapiBackend {
    fn new(probe: DriverProbe) -> Result<Self> {
        Ok(Self {
            descriptor: BackendDescriptor::new(BackendId::new(BACKEND_ID)?, "Linux VA-API")?,
            probe,
        })
    }
}

/// Probes the active Linux VA driver and builds a truthful registration.
pub fn registration() -> Result<Option<BackendRegistration>> {
    let Some(probe) = probe_driver()? else {
        return Ok(None);
    };
    let capabilities = capability_set(probe.capabilities.clone())?;
    if capabilities.is_empty() {
        return Ok(None);
    }
    Ok(Some(BackendRegistration::new(
        Arc::new(VaapiBackend::new(probe)?),
        capabilities,
        BACKEND_PRIORITY,
        BackendTier::Primary,
    )?))
}

impl MediaBackend for VaapiBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn probe_source(
        &self,
        _probe: &SourceProbe<'_>,
        operation: &OperationContext,
    ) -> Result<SourceProbeResult> {
        operation.check("probe_vaapi_source")?;
        Ok(SourceProbeResult::NoMatch)
    }

    fn open_source(
        &self,
        _request: &SourceRequest,
        operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>> {
        operation.check("open_vaapi_source")?;
        Err(unsupported(
            "open_vaapi_source",
            "the Linux VA-API codec backend does not open media containers",
        ))
    }

    fn create_decoder(
        &self,
        config: &DecoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Decoder>> {
        operation.check("create_vaapi_decoder")?;
        let codec = VideoCodec::from_id(config.stream().codec().as_str()).ok_or_else(|| {
            unsupported(
                "create_vaapi_decoder",
                "Linux VA-API supports H.264 and HEVC Main 8-bit decoding only",
            )
        })?;
        if config.stream().kind() != StreamKind::Video {
            return Err(conflict(
                "create_vaapi_decoder",
                "Linux VA-API video decoding requires a video stream",
            ));
        }
        let supported = match codec {
            VideoCodec::H264 => !self.probe.capabilities.h264_decode.is_empty(),
            VideoCodec::Hevc => self.probe.capabilities.hevc_decode,
        };
        if !supported {
            return Err(unsupported(
                "create_vaapi_decoder",
                "the active VA driver does not expose the requested decode profile",
            ));
        }
        validate_opaque_alpha(config.stream().metadata())?;
        Ok(Box::new(VaapiDecoder::new(
            config.clone(),
            codec,
            self.probe.render_node.clone(),
            self.probe.vendor.clone(),
        )?))
    }

    fn create_encoder(
        &self,
        config: &EncoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>> {
        operation.check("create_vaapi_encoder")?;
        encoder::create(self, config)
    }
}

/// Send and Sync owner for one decoded VA DMA-BUF frame.
#[derive(Clone)]
pub struct VaapiFrameBuffer {
    frame: Arc<DmaFrame>,
    width: u32,
    height: u32,
}

impl fmt::Debug for VaapiFrameBuffer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VaapiFrameBuffer")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("pixel_format", &PixelFormat::Nv12)
            .finish_non_exhaustive()
    }
}

impl VideoFrameBuffer for VaapiFrameBuffer {
    fn storage_kind(&self) -> superi_media_io::decode::FrameStorageKind {
        superi_media_io::decode::FrameStorageKind::External
    }

    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn pixel_format(&self) -> PixelFormat {
        PixelFormat::Nv12
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct NativeDecodedFrame {
    token: u64,
    frame: Arc<DmaFrame>,
    width: u32,
    height: u32,
}

enum DecoderCommand {
    Decode {
        token: u64,
        data: Vec<u8>,
        reply: SyncSender<std::result::Result<Vec<NativeDecodedFrame>, String>>,
    },
    Flush {
        reply: SyncSender<std::result::Result<Vec<NativeDecodedFrame>, String>>,
    },
    Reset {
        reply: SyncSender<std::result::Result<(), String>>,
    },
    Stop,
}

struct DecoderWorker {
    commands: SyncSender<DecoderCommand>,
    join: Option<JoinHandle<()>>,
}

impl DecoderWorker {
    fn spawn(render_node: PathBuf, codec: VideoCodec) -> Result<Self> {
        let (commands, receiver) = mpsc::sync_channel(8);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name(format!("superi-vaapi-{}-decode", codec.id()))
            .spawn(move || decoder_worker(render_node, codec, receiver, ready_sender))
            .map_err(|error| unavailable("spawn_vaapi_decoder", error.to_string()))?;
        match ready_receiver.recv() {
            Ok(Ok(())) => Ok(Self {
                commands,
                join: Some(join),
            }),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(unavailable("initialize_vaapi_decoder", error))
            }
            Err(error) => {
                let _ = join.join();
                Err(unavailable("initialize_vaapi_decoder", error.to_string()))
            }
        }
    }

    fn decode(&self, token: u64, data: Vec<u8>) -> Result<Vec<NativeDecodedFrame>> {
        let (reply, receiver) = mpsc::sync_channel(1);
        self.commands
            .send(DecoderCommand::Decode { token, data, reply })
            .map_err(|error| unavailable("send_vaapi_decode_command", error.to_string()))?;
        match receiver.recv() {
            Ok(Ok(frames)) => Ok(frames),
            Ok(Err(error)) => match error.strip_prefix(CORRUPT_NATIVE_PREFIX) {
                Some(detail) => Err(corrupt("decode_vaapi_packet", detail.to_owned())),
                None => Err(unavailable("receive_vaapi_decode_reply", error)),
            },
            Err(error) => Err(unavailable("receive_vaapi_decode_reply", error.to_string())),
        }
    }

    fn flush(&self) -> Result<Vec<NativeDecodedFrame>> {
        let (reply, receiver) = mpsc::sync_channel(1);
        self.commands
            .send(DecoderCommand::Flush { reply })
            .map_err(|error| unavailable("send_vaapi_flush_command", error.to_string()))?;
        receive_worker(receiver, "receive_vaapi_flush_reply")
    }

    fn reset(&self) -> Result<()> {
        let (reply, receiver) = mpsc::sync_channel(1);
        self.commands
            .send(DecoderCommand::Reset { reply })
            .map_err(|error| unavailable("send_vaapi_reset_command", error.to_string()))?;
        receive_worker(receiver, "receive_vaapi_reset_reply")
    }
}

impl Drop for DecoderWorker {
    fn drop(&mut self) {
        let _ = self.commands.send(DecoderCommand::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

struct VaapiDecoder {
    config: DecoderConfig,
    codec: VideoCodec,
    configuration: Arc<[u8]>,
    stream_color: Option<ColorSpace>,
    vendor: String,
    worker: DecoderWorker,
    timing: TimingLedger,
    output: VecDeque<NativeDecodedFrame>,
    lifecycle: CodecLifecycle,
    configuration_pending: bool,
}

impl VaapiDecoder {
    fn new(
        config: DecoderConfig,
        codec: VideoCodec,
        render_node: PathBuf,
        vendor: String,
    ) -> Result<Self> {
        let configuration = match config.stream().metadata().get("codec.configuration") {
            None => Arc::from([]),
            Some(MetadataValue::Bytes(bytes)) => Arc::clone(bytes),
            Some(_) => {
                return Err(corrupt(
                    "create_vaapi_decoder",
                    "codec.configuration metadata must contain bytes",
                ));
            }
        };
        let stream_color = declared_color(config.stream().metadata())?;
        Ok(Self {
            config,
            codec,
            configuration,
            stream_color,
            vendor,
            worker: DecoderWorker::spawn(render_node, codec)?,
            timing: TimingLedger::default(),
            output: VecDeque::new(),
            lifecycle: CodecLifecycle::default(),
            configuration_pending: true,
        })
    }

    fn frame(&mut self, native: NativeDecodedFrame) -> Result<VideoFrame> {
        let context = self.timing.remove(native.token)?;
        let timestamp = context
            .timing
            .presentation_time()
            .or_else(|| context.timing.decode_time())
            .ok_or_else(|| {
                corrupt(
                    "create_vaapi_frame",
                    "VA-API packet requires a presentation or decode timestamp",
                )
            })?;
        let duration = context.timing.duration().ok_or_else(|| {
            corrupt(
                "create_vaapi_frame",
                "VA-API packet requires an exact frame duration",
            )
        })?;
        let color = declared_color(&context.metadata)?
            .or(self.stream_color)
            .unwrap_or(ColorSpace::UNSPECIFIED);
        let buffer: Arc<dyn VideoFrameBuffer> = Arc::new(VaapiFrameBuffer {
            frame: native.frame,
            width: native.width,
            height: native.height,
        });
        let mut frame = VideoFrame::new(
            VideoFormat::new(
                native.width,
                native.height,
                PixelFormat::Nv12,
                color,
                AlphaMode::Opaque,
            )?,
            RationalTime::new(timestamp.value(), timestamp.timebase()),
            Duration::new(duration.value(), duration.timebase())?,
            buffer,
        )?;
        for (key, value) in context.metadata.iter() {
            frame = frame.with_metadata(key, value.clone())?;
        }
        frame = frame
            .with_metadata(
                "codec.platform-backend",
                MetadataValue::Text(BACKEND_ID.to_owned()),
            )?
            .with_metadata(
                "codec.platform-vendor",
                MetadataValue::Text(self.vendor.clone()),
            )?
            .with_metadata("video.keyframe", MetadataValue::Boolean(context.keyframe))?;
        Ok(frame)
    }
}

impl Decoder for VaapiDecoder {
    fn config(&self) -> &DecoderConfig {
        &self.config
    }

    fn send_packet(&mut self, packet: Packet, operation: &OperationContext) -> Result<()> {
        operation.check("send_vaapi_packet")?;
        self.lifecycle.ensure_accepting("send_vaapi_packet")?;
        if packet.stream_id() != self.config.stream().id() {
            return Err(conflict(
                "send_vaapi_packet",
                "VA-API packet stream does not match decoder configuration",
            ));
        }
        if packet.timing().timebase() != self.config.stream().timebase() {
            return Err(corrupt(
                "send_vaapi_packet",
                "VA-API packet timebase does not match its stream",
            ));
        }
        if packet.timing().presentation_time().is_none() && packet.timing().decode_time().is_none()
        {
            return Err(corrupt(
                "send_vaapi_packet",
                "VA-API packet requires a presentation or decode timestamp",
            ));
        }
        if packet.timing().duration().is_none() {
            return Err(corrupt(
                "send_vaapi_packet",
                "VA-API packet requires an exact frame duration",
            ));
        }
        if self.timing.pending.len() >= MAX_PENDING_PACKETS {
            return Err(categorized(
                "send_vaapi_packet",
                ErrorCategory::ResourceExhausted,
                Recoverability::Retryable,
                "VA-API decoder packet queue is full and must be drained",
            ));
        }
        validate_opaque_alpha(packet.metadata())?;
        let include_parameter_sets = self.configuration_pending || packet.is_keyframe();
        let normalized = match self.codec {
            VideoCodec::H264 => normalize_avc_access_unit(
                &self.configuration,
                packet.data(),
                include_parameter_sets,
            )?,
            VideoCodec::Hevc => normalize_hevc_access_unit(
                &self.configuration,
                packet.data(),
                include_parameter_sets,
            )?,
        };
        let token = self.timing.insert(
            packet.timing(),
            packet.is_keyframe(),
            packet.metadata().clone(),
        )?;
        match self.worker.decode(token, normalized) {
            Ok(frames) => self.output.extend(frames),
            Err(error) => {
                let _ = self.timing.remove(token);
                return Err(error);
            }
        }
        self.configuration_pending = false;
        operation.check("send_vaapi_packet")?;
        Ok(())
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<DecodeOutput> {
        operation.check("receive_vaapi_frame")?;
        if let Some(native) = self.output.pop_front() {
            return Ok(DecodeOutput::Frame(self.frame(native)?));
        }
        if self.lifecycle.ended() {
            Ok(DecodeOutput::EndOfStream)
        } else {
            Ok(DecodeOutput::NeedInput)
        }
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("flush_vaapi_decoder")?;
        if self.lifecycle.ended() {
            return Ok(());
        }
        self.lifecycle.begin_flush();
        self.output.extend(self.worker.flush()?);
        self.lifecycle.finish_flush();
        operation.check("flush_vaapi_decoder")?;
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("reset_vaapi_decoder")?;
        self.worker.reset()?;
        self.output.clear();
        self.timing.clear();
        self.lifecycle.reset();
        self.configuration_pending = true;
        Ok(())
    }
}

fn decoder_worker(
    render_node: PathBuf,
    codec: VideoCodec,
    commands: Receiver<DecoderCommand>,
    ready: SyncSender<std::result::Result<(), String>>,
) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        create_native_decoder(&render_node, codec)
    }))
    .map_err(panic_message)
    .and_then(|result| result);
    let (mut decoder, mut pool) = match result {
        Ok(value) => {
            let _ = ready.send(Ok(()));
            value
        }
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };

    while let Ok(command) = commands.recv() {
        match command {
            DecoderCommand::Decode { token, data, reply } => {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    drive_decode(&mut decoder, &mut pool, token, &data)
                }))
                .map_err(panic_message)
                .and_then(|result| result);
                let _ = reply.send(result);
            }
            DecoderCommand::Flush { reply } => {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    decoder
                        .flush()
                        .map_err(|error| error.to_string())
                        .and_then(|()| drain_decoder_events(&mut decoder, &mut pool))
                }))
                .map_err(panic_message)
                .and_then(|result| result);
                let _ = reply.send(result);
            }
            DecoderCommand::Reset { reply } => {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    create_native_decoder(&render_node, codec)
                }))
                .map_err(panic_message)
                .and_then(|result| result)
                .map(|(replacement_decoder, replacement_pool)| {
                    decoder = replacement_decoder;
                    pool = replacement_pool;
                });
                let _ = reply.send(result);
            }
            DecoderCommand::Stop => break,
        }
    }
}

fn create_native_decoder(
    render_node: &Path,
    codec: VideoCodec,
) -> std::result::Result<
    (
        DynStatelessVideoDecoder<DmaFrame>,
        FramePool<GenericDmaVideoFrame>,
    ),
    String,
> {
    let display = Display::open_drm_display(render_node).map_err(|error| error.to_string())?;
    let gbm = GbmDevice::open(render_node)?;
    let pool = FramePool::new(move |info| {
        Arc::clone(&gbm)
            .new_frame(
                Fourcc::from(b"NV12"),
                info.display_resolution,
                info.coded_resolution,
                GbmUsage::Decode,
            )
            .and_then(|frame| frame.to_generic_dma_video_frame())
            .expect("probed GBM device failed to allocate a decode frame")
    });
    let decoder = match codec {
        VideoCodec::H264 => {
            let decoder: StatelessDecoder<H264, CrosVaapiDecoderBackend<DmaFrame>> =
                StatelessDecoder::<H264, CrosVaapiDecoderBackend<DmaFrame>>::new_vaapi(
                    Rc::clone(&display),
                    BlockingMode::Blocking,
                )
                .map_err(|error| error.to_string())?;
            decoder.into_trait_object()
        }
        VideoCodec::Hevc => {
            let decoder: StatelessDecoder<H265, CrosVaapiDecoderBackend<DmaFrame>> =
                StatelessDecoder::<H265, CrosVaapiDecoderBackend<DmaFrame>>::new_vaapi(
                    Rc::clone(&display),
                    BlockingMode::Blocking,
                )
                .map_err(|error| error.to_string())?;
            decoder.into_trait_object()
        }
    };
    Ok((decoder, pool))
}

fn drive_decode(
    decoder: &mut DynStatelessVideoDecoder<DmaFrame>,
    pool: &mut FramePool<GenericDmaVideoFrame>,
    token: u64,
    data: &[u8],
) -> std::result::Result<Vec<NativeDecodedFrame>, String> {
    let mut offset = 0;
    let mut frames = Vec::new();
    while offset < data.len() {
        match decoder.decode(token, &data[offset..], &mut || pool.alloc()) {
            Ok(0) => return Err("VA-API decoder consumed zero bytes".to_owned()),
            Ok(processed) => {
                offset = offset
                    .checked_add(processed)
                    .ok_or_else(|| "VA-API decoder byte offset overflowed".to_owned())?;
                frames.extend(drain_decoder_events(decoder, pool)?);
            }
            Err(DecodeError::CheckEvents) | Err(DecodeError::NotEnoughOutputBuffers(_)) => {
                let events = drain_decoder_events(decoder, pool)?;
                if events.is_empty() {
                    return Err(
                        "VA-API decoder needs an output buffer still owned by a consumer"
                            .to_owned(),
                    );
                }
                frames.extend(events);
            }
            Err(DecodeError::ParseFrameError(error)) => {
                return Err(format!("{CORRUPT_NATIVE_PREFIX}{error}"));
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(frames)
}

fn drain_decoder_events(
    decoder: &mut DynStatelessVideoDecoder<DmaFrame>,
    pool: &mut FramePool<GenericDmaVideoFrame>,
) -> std::result::Result<Vec<NativeDecodedFrame>, String> {
    let mut frames = Vec::new();
    while let Some(event) = decoder.next_event() {
        match event {
            DecoderEvent::FormatChanged => {
                let info = decoder
                    .stream_info()
                    .ok_or_else(|| "VA-API format change did not expose stream info".to_owned())?;
                if info.format != DecodedFormat::NV12 {
                    return Err(format!(
                        "VA-API negotiated unsupported output format {:?}",
                        info.format
                    ));
                }
                pool.resize(info);
            }
            DecoderEvent::FrameReady(handle) => {
                handle.sync().map_err(|error| error.to_string())?;
                let resolution = handle.display_resolution();
                frames.push(NativeDecodedFrame {
                    token: handle.timestamp(),
                    frame: handle.video_frame(),
                    width: resolution.width,
                    height: resolution.height,
                });
            }
        }
    }
    Ok(frames)
}

fn probe_driver() -> Result<Option<DriverProbe>> {
    let override_selected = env::var_os(RENDER_NODE_ENV).is_some();
    for render_node in render_nodes()? {
        let display = match Display::open_drm_display(&render_node) {
            Ok(display) => display,
            Err(_) => continue,
        };
        if GbmDevice::open(&render_node).is_err() {
            continue;
        }
        let mut capabilities = match query_capabilities(&display) {
            Ok(capabilities) => capabilities,
            Err(error) if override_selected => {
                return Err(unavailable_with_node(
                    "probe_vaapi_capabilities",
                    error,
                    &render_node,
                ));
            }
            Err(_) => continue,
        };
        if !decoder_is_constructible(&render_node, VideoCodec::H264) {
            capabilities.h264_decode.clear();
        }
        capabilities.hevc_decode &= decoder_is_constructible(&render_node, VideoCodec::Hevc);
        if capabilities == DriverCapabilities::default() {
            continue;
        }
        let vendor = display
            .query_vendor_string()
            .unwrap_or_else(|_| "unknown VA-API vendor".to_owned());
        return Ok(Some(DriverProbe {
            render_node,
            vendor,
            capabilities,
        }));
    }
    Ok(None)
}

fn decoder_is_constructible(render_node: &Path, codec: VideoCodec) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        create_native_decoder(render_node, codec)
    }))
    .is_ok_and(|result| result.is_ok())
}

fn render_nodes() -> Result<Vec<PathBuf>> {
    if let Some(value) = env::var_os(RENDER_NODE_ENV) {
        let path = PathBuf::from(value);
        if !path.is_absolute() {
            return Err(categorized(
                "read_vaapi_render_node_override",
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "SUPERI_VAAPI_RENDER_NODE must be an absolute path",
            ));
        }
        return Ok(vec![path]);
    }
    render_nodes_in(Path::new("/dev/dri"))
}

fn render_nodes_in(directory: &Path) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(unavailable("list_vaapi_render_nodes", error.to_string())),
    };
    let mut nodes = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.strip_prefix("renderD")
                        .is_some_and(|suffix| suffix.bytes().all(|byte| byte.is_ascii_digit()))
                })
        })
        .collect::<Vec<_>>();
    nodes.sort();
    Ok(nodes)
}

fn query_capabilities(display: &Display) -> std::result::Result<DriverCapabilities, String> {
    let profiles = display
        .query_config_profiles()
        .map_err(|error| error.to_string())?;
    let h264_profiles = [
        (
            H264Profile::ConstrainedBaseline,
            VAProfile::VAProfileH264ConstrainedBaseline,
        ),
        (H264Profile::Main, VAProfile::VAProfileH264Main),
        (H264Profile::High, VAProfile::VAProfileH264High),
    ];
    let h264_decode = h264_profiles
        .iter()
        .copied()
        .filter(|(_, native)| profiles.contains(native))
        .filter(|(_, native)| {
            supports_config(display, *native, VAEntrypoint::VAEntrypointVLD, false)
        })
        .map(|(profile, _)| profile)
        .collect::<BTreeSet<_>>();
    let h264_bootstrap = h264_decode.contains(&H264Profile::Main);
    let hevc_decode = h264_bootstrap
        && profiles.contains(&VAProfile::VAProfileHEVCMain)
        && supports_config(
            display,
            VAProfile::VAProfileHEVCMain,
            VAEntrypoint::VAEntrypointVLD,
            false,
        );
    let h264_encode = h264_profiles
        .iter()
        .copied()
        .filter(|(_, native)| profiles.contains(native))
        .filter(|(_, native)| {
            supports_config(display, *native, VAEntrypoint::VAEntrypointEncSlice, true)
                || supports_config(display, *native, VAEntrypoint::VAEntrypointEncSliceLP, true)
        })
        .map(|(profile, _)| profile)
        .collect::<BTreeSet<_>>();
    Ok(DriverCapabilities {
        h264_decode,
        hevc_decode,
        h264_encode,
    })
}

fn supports_config(display: &Display, profile: i32, entrypoint: u32, require_cbr: bool) -> bool {
    let has_entrypoint = display
        .query_config_entrypoints(profile)
        .is_ok_and(|entrypoints| entrypoints.contains(&entrypoint));
    if !has_entrypoint {
        return false;
    }
    let mut attributes = [
        cros_codecs::libva::VAConfigAttrib {
            type_: cros_codecs::libva::VAConfigAttribType::VAConfigAttribRTFormat,
            value: 0,
        },
        cros_codecs::libva::VAConfigAttrib {
            type_: cros_codecs::libva::VAConfigAttribType::VAConfigAttribRateControl,
            value: 0,
        },
    ];
    if display
        .get_config_attributes(profile, entrypoint, &mut attributes)
        .is_err()
    {
        return false;
    }
    let render_format = attributes[0].value;
    let supports_nv12 = render_format != cros_codecs::libva::VA_ATTRIB_NOT_SUPPORTED
        && render_format & cros_codecs::libva::VA_RT_FORMAT_YUV420 != 0;
    let rate_control = attributes[1].value;
    let supports_cbr = rate_control != cros_codecs::libva::VA_ATTRIB_NOT_SUPPORTED
        && rate_control & cros_codecs::libva::VA_RC_CBR != 0;
    supports_nv12 && (!require_cbr || supports_cbr)
}

fn declared_color(metadata: &MediaMetadata) -> Result<Option<ColorSpace>> {
    let primaries = text_metadata(metadata, "video.color-primaries")?;
    let transfer = text_metadata(metadata, "video.transfer-function")?;
    let matrix = text_metadata(metadata, "video.matrix-coefficients")?;
    let range = text_metadata(metadata, "video.color-range")?;
    match (primaries, transfer, matrix, range) {
        (None, None, None, None) => Ok(None),
        (Some(primaries), Some(transfer), Some(matrix), Some(range)) => {
            let primaries = ColorPrimaries::from_code(primaries).ok_or_else(|| {
                corrupt("read_vaapi_color", "unknown video color primaries metadata")
            })?;
            let transfer = TransferFunction::from_code(transfer).ok_or_else(|| {
                corrupt(
                    "read_vaapi_color",
                    "unknown video transfer function metadata",
                )
            })?;
            let matrix = MatrixCoefficients::from_code(matrix)
                .ok_or_else(|| corrupt("read_vaapi_color", "unknown video matrix metadata"))?;
            let range = ColorRange::from_code(range)
                .ok_or_else(|| corrupt("read_vaapi_color", "unknown video color range metadata"))?;
            Ok(Some(ColorSpace::new(primaries, transfer, matrix, range)))
        }
        _ => Err(corrupt(
            "read_vaapi_color",
            "video color metadata must declare every interpretation axis",
        )),
    }
}

fn text_metadata<'a>(metadata: &'a MediaMetadata, key: &str) -> Result<Option<&'a str>> {
    match metadata.get(key) {
        None => Ok(None),
        Some(MetadataValue::Text(value)) => Ok(Some(value)),
        Some(_) => Err(corrupt(
            "read_vaapi_color",
            format!("{key} metadata must contain text"),
        )),
    }
}

fn receive_worker<T>(
    receiver: Receiver<std::result::Result<T, String>>,
    operation: &'static str,
) -> Result<T> {
    receiver
        .recv()
        .map_err(|error| unavailable(operation, error.to_string()))?
        .map_err(|error| unavailable(operation, error))
}

fn unavailable(operation: &'static str, message: impl Into<String>) -> Error {
    categorized(
        operation,
        ErrorCategory::Unavailable,
        Recoverability::Degraded,
        message,
    )
}

fn unavailable_with_node(
    operation: &'static str,
    message: impl Into<String>,
    render_node: &Path,
) -> Error {
    unavailable(operation, message).with_context(
        ErrorContext::new("superi-codecs-platform.vaapi", operation)
            .with_field("render_node", render_node.display().to_string()),
    )
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        format!("native VA-API dependency panicked: {message}")
    } else if let Some(message) = payload.downcast_ref::<&'static str>() {
        format!("native VA-API dependency panicked: {message}")
    } else {
        "native VA-API dependency panicked without a message".to_owned()
    }
}

mod encoder {
    use super::*;
    use cros_codecs::backend::vaapi::encoder::VaapiBackend as CrosVaapiEncoderBackend;
    use cros_codecs::codec::h264::parser::{Level, Profile};
    use cros_codecs::encoder::h264::EncoderConfig as CrosH264Config;
    use cros_codecs::encoder::stateless::h264::StatelessEncoder as CrosH264Encoder;
    use cros_codecs::encoder::{
        FrameMetadata, PredictionStructure, RateControl, Tunings, VideoEncoder as CrosVideoEncoder,
    };
    use cros_codecs::libva::Surface;
    use cros_codecs::{FrameLayout, PlaneLayout, Resolution};

    type NativeEncoder = Box<dyn CrosVideoEncoder<VaapiEncodeFrame>>;

    struct Nv12Pixels {
        width: u32,
        height: u32,
        luma: Vec<u8>,
        chroma: Vec<u8>,
    }

    enum EncoderInputData {
        Shared {
            frame: Arc<DmaFrame>,
            width: u32,
            height: u32,
        },
        Cpu(Nv12Pixels),
    }

    #[derive(Debug)]
    enum VaapiEncodeFrame {
        Shared(Arc<DmaFrame>),
        Owned(GenericDmaVideoFrame),
    }

    impl CrosVideoFrame for VaapiEncodeFrame {
        type MemDescriptor = GenericDmaVideoFrame;
        type NativeHandle = Surface<GenericDmaVideoFrame>;

        fn fourcc(&self) -> Fourcc {
            match self {
                Self::Shared(frame) => frame.fourcc(),
                Self::Owned(frame) => frame.fourcc(),
            }
        }

        fn resolution(&self) -> Resolution {
            match self {
                Self::Shared(frame) => frame.resolution(),
                Self::Owned(frame) => frame.resolution(),
            }
        }

        fn get_plane_size(&self) -> Vec<usize> {
            match self {
                Self::Shared(frame) => frame.get_plane_size(),
                Self::Owned(frame) => frame.get_plane_size(),
            }
        }

        fn get_plane_pitch(&self) -> Vec<usize> {
            match self {
                Self::Shared(frame) => frame.get_plane_pitch(),
                Self::Owned(frame) => frame.get_plane_pitch(),
            }
        }

        fn map<'a>(
            &'a self,
        ) -> std::result::Result<Box<dyn cros_codecs::video_frame::ReadMapping<'a> + 'a>, String>
        {
            match self {
                Self::Shared(frame) => frame.map(),
                Self::Owned(frame) => frame.map(),
            }
        }

        fn map_mut<'a>(
            &'a mut self,
        ) -> std::result::Result<Box<dyn cros_codecs::video_frame::WriteMapping<'a> + 'a>, String>
        {
            match self {
                Self::Shared(_) => {
                    Err("shared VA-API encode input cannot be mapped mutably".to_owned())
                }
                Self::Owned(frame) => frame.map_mut(),
            }
        }

        fn to_native_handle(
            &self,
            display: &Rc<Display>,
        ) -> std::result::Result<Self::NativeHandle, String> {
            match self {
                Self::Shared(frame) => frame.to_native_handle(display),
                Self::Owned(frame) => frame.to_native_handle(display),
            }
        }
    }

    struct NativeEncodedPacket {
        token: u64,
        data: Vec<u8>,
    }

    enum EncoderCommand {
        Encode {
            token: u64,
            input: EncoderInputData,
            force_keyframe: bool,
            framerate: u32,
            reply: SyncSender<std::result::Result<Vec<NativeEncodedPacket>, String>>,
        },
        Flush {
            reply: SyncSender<std::result::Result<Vec<NativeEncodedPacket>, String>>,
        },
        Reset {
            reply: SyncSender<std::result::Result<(), String>>,
        },
        Stop,
    }

    struct EncoderWorker {
        commands: SyncSender<EncoderCommand>,
        join: Option<JoinHandle<()>>,
    }

    impl EncoderWorker {
        fn spawn(render_node: PathBuf, width: u32, height: u32, bitrate: u64) -> Result<Self> {
            let (commands, receiver) = mpsc::sync_channel(8);
            let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
            let join = thread::Builder::new()
                .name("superi-vaapi-h264-encode".to_owned())
                .spawn(move || {
                    encoder_worker(render_node, width, height, bitrate, receiver, ready_sender)
                })
                .map_err(|error| unavailable("spawn_vaapi_encoder", error.to_string()))?;
            match ready_receiver.recv() {
                Ok(Ok(())) => Ok(Self {
                    commands,
                    join: Some(join),
                }),
                Ok(Err(error)) => {
                    let _ = join.join();
                    Err(unavailable("initialize_vaapi_encoder", error))
                }
                Err(error) => {
                    let _ = join.join();
                    Err(unavailable("initialize_vaapi_encoder", error.to_string()))
                }
            }
        }

        fn encode(
            &self,
            token: u64,
            input: EncoderInputData,
            force_keyframe: bool,
            framerate: u32,
        ) -> Result<Vec<NativeEncodedPacket>> {
            let (reply, receiver) = mpsc::sync_channel(1);
            self.commands
                .send(EncoderCommand::Encode {
                    token,
                    input,
                    force_keyframe,
                    framerate,
                    reply,
                })
                .map_err(|error| unavailable("send_vaapi_encode_command", error.to_string()))?;
            receive_worker(receiver, "receive_vaapi_encode_reply")
        }

        fn flush(&self) -> Result<Vec<NativeEncodedPacket>> {
            let (reply, receiver) = mpsc::sync_channel(1);
            self.commands
                .send(EncoderCommand::Flush { reply })
                .map_err(|error| unavailable("send_vaapi_encode_flush", error.to_string()))?;
            receive_worker(receiver, "receive_vaapi_encode_flush")
        }

        fn reset(&self) -> Result<()> {
            let (reply, receiver) = mpsc::sync_channel(1);
            self.commands
                .send(EncoderCommand::Reset { reply })
                .map_err(|error| unavailable("send_vaapi_encode_reset", error.to_string()))?;
            receive_worker(receiver, "receive_vaapi_encode_reset")
        }
    }

    impl Drop for EncoderWorker {
        fn drop(&mut self) {
            let _ = self.commands.send(EncoderCommand::Stop);
            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        }
    }

    struct VaapiEncoder {
        config: EncoderConfig,
        format: VideoFormat,
        vendor: String,
        worker: EncoderWorker,
        timing: TimingLedger,
        output: VecDeque<NativeEncodedPacket>,
        lifecycle: CodecLifecycle,
        frame_index: u64,
        configuration: Arc<[u8]>,
    }

    impl VaapiEncoder {
        fn new(backend: &VaapiBackend, config: EncoderConfig, format: VideoFormat) -> Result<Self> {
            let bitrate = target_bitrate(format)?;
            Ok(Self {
                worker: EncoderWorker::spawn(
                    backend.probe.render_node.clone(),
                    format.width(),
                    format.height(),
                    bitrate,
                )?,
                config,
                format,
                vendor: backend.probe.vendor.clone(),
                timing: TimingLedger::default(),
                output: VecDeque::new(),
                lifecycle: CodecLifecycle::default(),
                frame_index: 0,
                configuration: Arc::from([]),
            })
        }

        fn packet(&mut self, native: NativeEncodedPacket) -> Result<Packet> {
            let context = self.timing.remove(native.token)?;
            let keyframe = h264_is_keyframe(&native.data);
            if let Some(configuration) = h264_avcc_configuration(&native.data)? {
                self.configuration = Arc::from(configuration);
            }
            let mut packet = Packet::new(
                self.config.stream_id(),
                Arc::from(native.data),
                context.timing,
            )
            .with_keyframe(keyframe);
            for (key, value) in context.metadata.iter() {
                packet = packet.with_metadata(key, value.clone())?;
            }
            packet = packet
                .with_metadata(
                    "codec.platform-backend",
                    MetadataValue::Text(BACKEND_ID.to_owned()),
                )?
                .with_metadata(
                    "codec.platform-vendor",
                    MetadataValue::Text(self.vendor.clone()),
                )?;
            if !self.configuration.is_empty() {
                packet = packet.with_metadata(
                    "codec.configuration",
                    MetadataValue::Bytes(Arc::clone(&self.configuration)),
                )?;
            }
            Ok(packet)
        }
    }

    pub(super) fn create(
        backend: &VaapiBackend,
        config: &EncoderConfig,
    ) -> Result<Box<dyn Encoder>> {
        if config.codec().as_str() != H264_CODEC_ID
            || backend.probe.capabilities.h264_encode.is_empty()
        {
            return Err(unsupported(
                "create_vaapi_encoder",
                "the active VA driver does not expose H.264 encoding",
            ));
        }
        let EncoderMediaFormat::Video(format) = config.media_format() else {
            return Err(conflict(
                "create_vaapi_encoder",
                "Linux VA-API H.264 export requires video input",
            ));
        };
        if format.pixel_format() != PixelFormat::Nv12 || format.alpha_mode() != AlphaMode::Opaque {
            return Err(unsupported(
                "create_vaapi_encoder",
                "Linux VA-API H.264 export requires opaque NV12 input",
            ));
        }
        if format.color_space() != ColorSpace::UNSPECIFIED {
            return Err(unsupported(
                "create_vaapi_encoder",
                "Linux VA-API H.264 export cannot preserve declared color signaling yet",
            ));
        }
        Ok(Box::new(VaapiEncoder::new(
            backend,
            config.clone(),
            *format,
        )?))
    }

    impl Encoder for VaapiEncoder {
        fn config(&self) -> &EncoderConfig {
            &self.config
        }

        fn send(&mut self, input: EncodeInput, operation: &OperationContext) -> Result<()> {
            operation.check("send_vaapi_frame")?;
            self.lifecycle.ensure_accepting("send_vaapi_frame")?;
            let EncodeInput::Video(frame) = input else {
                return Err(conflict(
                    "send_vaapi_frame",
                    "Linux VA-API H.264 export accepts video frames only",
                ));
            };
            if frame.format() != self.format {
                return Err(conflict(
                    "send_vaapi_frame",
                    "VA-API input frame format does not match encoder configuration",
                ));
            }
            if frame.timestamp().timebase() != self.config.timebase()
                || frame.duration().timebase() != self.config.timebase()
            {
                return Err(conflict(
                    "send_vaapi_frame",
                    "VA-API input frame timing does not match encoder timebase",
                ));
            }
            if frame.duration().value() == 0 {
                return Err(corrupt(
                    "send_vaapi_frame",
                    "VA-API input frame duration must be greater than zero",
                ));
            }
            validate_opaque_alpha(frame.metadata())?;
            let encoder_input = encoder_input(&frame)?;
            let timing = PacketTiming::new(
                self.config.timebase(),
                Some(frame.timestamp().value()),
                Some(frame.timestamp().value()),
                Some(frame.duration().value()),
            )?;
            let force_keyframe = self.frame_index == 0
                || matches!(
                    frame.metadata().get("video.force-keyframe"),
                    Some(MetadataValue::Boolean(true))
                );
            let token = self
                .timing
                .insert(timing, force_keyframe, frame.metadata().clone())?;
            let framerate = frame_rate(self.config.timebase(), frame.duration().value())?;
            match self
                .worker
                .encode(token, encoder_input, force_keyframe, framerate)
            {
                Ok(packets) => self.output.extend(packets),
                Err(error) => {
                    let _ = self.timing.remove(token);
                    return Err(error);
                }
            }
            self.frame_index = self.frame_index.checked_add(1).ok_or_else(|| {
                categorized(
                    "send_vaapi_frame",
                    ErrorCategory::ResourceExhausted,
                    Recoverability::Terminal,
                    "VA-API encoder frame counter overflowed",
                )
            })?;
            operation.check("send_vaapi_frame")?;
            Ok(())
        }

        fn receive(&mut self, operation: &OperationContext) -> Result<EncodeOutput> {
            operation.check("receive_vaapi_packet")?;
            if let Some(native) = self.output.pop_front() {
                return Ok(EncodeOutput::Packet(self.packet(native)?));
            }
            if self.lifecycle.ended() {
                Ok(EncodeOutput::EndOfStream)
            } else {
                Ok(EncodeOutput::NeedInput)
            }
        }

        fn flush(&mut self, operation: &OperationContext) -> Result<()> {
            operation.check("flush_vaapi_encoder")?;
            if self.lifecycle.ended() {
                return Ok(());
            }
            self.lifecycle.begin_flush();
            self.output.extend(self.worker.flush()?);
            self.lifecycle.finish_flush();
            operation.check("flush_vaapi_encoder")?;
            Ok(())
        }

        fn reset(&mut self, operation: &OperationContext) -> Result<()> {
            operation.check("reset_vaapi_encoder")?;
            self.worker.reset()?;
            self.timing.clear();
            self.output.clear();
            self.lifecycle.reset();
            self.frame_index = 0;
            self.configuration = Arc::from([]);
            Ok(())
        }
    }

    fn encoder_worker(
        render_node: PathBuf,
        width: u32,
        height: u32,
        bitrate: u64,
        commands: Receiver<EncoderCommand>,
        ready: SyncSender<std::result::Result<(), String>>,
    ) {
        let display = match Display::open_drm_display(&render_node) {
            Ok(display) => display,
            Err(error) => {
                let _ = ready.send(Err(error.to_string()));
                return;
            }
        };
        let gbm = match GbmDevice::open(&render_node) {
            Ok(gbm) => gbm,
            Err(error) => {
                let _ = ready.send(Err(error));
                return;
            }
        };
        let initial = catch_unwind(AssertUnwindSafe(|| {
            create_native_encoder(Rc::clone(&display), width, height, bitrate)
        }))
        .map_err(panic_message)
        .and_then(|result| result);
        let mut native = match initial {
            Ok(native) => {
                let _ = ready.send(Ok(()));
                native
            }
            Err(error) => {
                let _ = ready.send(Err(error));
                return;
            }
        };
        while let Ok(command) = commands.recv() {
            match command {
                EncoderCommand::Encode {
                    token,
                    input,
                    force_keyframe,
                    framerate,
                    reply,
                } => {
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        encode_one(
                            &mut native,
                            Arc::clone(&gbm),
                            token,
                            input,
                            force_keyframe,
                            framerate,
                            bitrate,
                        )
                    }))
                    .map_err(panic_message)
                    .and_then(|result| result);
                    let _ = reply.send(result);
                }
                EncoderCommand::Flush { reply } => {
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        native
                            .drain()
                            .map_err(|error| error.to_string())
                            .and_then(|()| poll_encoded(&mut native))
                    }))
                    .map_err(panic_message)
                    .and_then(|result| result);
                    let _ = reply.send(result);
                }
                EncoderCommand::Reset { reply } => {
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        create_native_encoder(Rc::clone(&display), width, height, bitrate)
                    }))
                    .map_err(panic_message)
                    .and_then(|result| result)
                    .map(|replacement| native = replacement);
                    let _ = reply.send(result);
                }
                EncoderCommand::Stop => break,
            }
        }
    }

    fn create_native_encoder(
        display: Rc<Display>,
        width: u32,
        height: u32,
        bitrate: u64,
    ) -> std::result::Result<NativeEncoder, String> {
        let resolution = Resolution::from((width, height));
        let profiles = [Profile::Main, Profile::High, Profile::Baseline];
        let mut last_error = "VA-API rejected every H.264 encode profile".to_owned();
        for profile in profiles {
            for low_power in [false, true] {
                let config = CrosH264Config {
                    resolution,
                    profile,
                    level: Level::L5_2,
                    pred_structure: PredictionStructure::LowDelay { limit: 240 },
                    initial_tunings: Tunings {
                        rate_control: RateControl::ConstantBitrate(bitrate),
                        framerate: 30,
                        min_quality: 0,
                        max_quality: 51,
                    },
                };
                type Concrete = CrosH264Encoder<
                    VaapiEncodeFrame,
                    CrosVaapiEncoderBackend<GenericDmaVideoFrame, Surface<GenericDmaVideoFrame>>,
                >;
                match Concrete::new_vaapi(
                    Rc::clone(&display),
                    config,
                    Fourcc::from(b"NV12"),
                    resolution,
                    low_power,
                    BlockingMode::Blocking,
                ) {
                    Ok(encoder) => return Ok(Box::new(encoder)),
                    Err(error) => last_error = error.to_string(),
                }
            }
        }
        Err(last_error)
    }

    fn encode_one(
        native: &mut NativeEncoder,
        gbm: Arc<GbmDevice>,
        token: u64,
        input: EncoderInputData,
        force_keyframe: bool,
        framerate: u32,
        bitrate: u64,
    ) -> std::result::Result<Vec<NativeEncodedPacket>, String> {
        native
            .tune(Tunings {
                rate_control: RateControl::ConstantBitrate(bitrate),
                framerate,
                min_quality: 0,
                max_quality: 51,
            })
            .map_err(|error| error.to_string())?;
        let (frame, width, height) = match input {
            EncoderInputData::Shared {
                frame,
                width,
                height,
            } => (VaapiEncodeFrame::Shared(frame), width, height),
            EncoderInputData::Cpu(pixels) => {
                let resolution = Resolution::from((pixels.width, pixels.height));
                let mut gbm_frame = gbm.new_frame(
                    Fourcc::from(b"NV12"),
                    resolution,
                    resolution,
                    GbmUsage::Encode,
                )?;
                copy_to_native(&mut gbm_frame, &pixels)?;
                (
                    VaapiEncodeFrame::Owned(gbm_frame.to_generic_dma_video_frame()?),
                    pixels.width,
                    pixels.height,
                )
            }
        };
        let pitches = frame.get_plane_pitch();
        if pitches.len() != 2 {
            return Err("VA-API encoder input must contain two NV12 planes".to_owned());
        }
        let luma_size = pitches[0]
            .checked_mul(height as usize)
            .ok_or_else(|| "VA-API frame layout overflowed".to_owned())?;
        let layout = FrameLayout {
            format: (Fourcc::from(b"NV12"), 0),
            size: Resolution::from((width, height)),
            planes: vec![
                PlaneLayout {
                    buffer_index: 0,
                    offset: 0,
                    stride: pitches[0],
                },
                PlaneLayout {
                    buffer_index: 0,
                    offset: luma_size,
                    stride: pitches[1],
                },
            ],
        };
        native
            .encode(
                FrameMetadata {
                    timestamp: token,
                    layout,
                    force_keyframe,
                },
                frame,
            )
            .map_err(|error| error.to_string())?;
        poll_encoded(native)
    }

    fn poll_encoded(
        native: &mut NativeEncoder,
    ) -> std::result::Result<Vec<NativeEncodedPacket>, String> {
        let mut packets = Vec::new();
        while let Some(packet) = native.poll().map_err(|error| error.to_string())? {
            packets.push(NativeEncodedPacket {
                token: packet.metadata.timestamp,
                data: packet.bitstream,
            });
        }
        Ok(packets)
    }

    fn encoder_input(frame: &VideoFrame) -> Result<EncoderInputData> {
        if let Some(vaapi) = frame.buffer().as_any().downcast_ref::<VaapiFrameBuffer>() {
            return Ok(EncoderInputData::Shared {
                frame: Arc::clone(&vaapi.frame),
                width: frame.format().width(),
                height: frame.format().height(),
            });
        }
        let width = usize::try_from(frame.format().width()).map_err(|_| {
            corrupt(
                "read_vaapi_nv12",
                "NV12 width cannot be represented on this platform",
            )
        })?;
        let height = usize::try_from(frame.format().height()).map_err(|_| {
            corrupt(
                "read_vaapi_nv12",
                "NV12 height cannot be represented on this platform",
            )
        })?;
        let chroma_rows = height.div_ceil(2);
        let cpu = frame
            .buffer()
            .as_any()
            .downcast_ref::<CpuVideoBuffer>()
            .ok_or_else(|| {
                unsupported(
                    "read_vaapi_nv12",
                    "VA-API H.264 export accepts CPU NV12 or Linux VA-API NV12 storage",
                )
            })?;
        let planes = cpu.planes();
        if planes.len() != 2 {
            return Err(corrupt(
                "read_vaapi_nv12",
                "NV12 CPU input must contain two planes",
            ));
        }
        Ok(EncoderInputData::Cpu(Nv12Pixels {
            width: frame.format().width(),
            height: frame.format().height(),
            luma: copy_visible_rows(planes[0].bytes(), planes[0].stride(), width, height)?,
            chroma: copy_visible_rows(planes[1].bytes(), planes[1].stride(), width, chroma_rows)?,
        }))
    }

    fn copy_visible_rows(
        source: &[u8],
        source_stride: usize,
        row_bytes: usize,
        rows: usize,
    ) -> Result<Vec<u8>> {
        if source_stride < row_bytes {
            return Err(corrupt(
                "read_vaapi_nv12",
                "NV12 source stride is smaller than its visible row",
            ));
        }
        let required = source_stride
            .checked_mul(rows)
            .ok_or_else(|| corrupt("read_vaapi_nv12", "NV12 source plane size overflowed"))?;
        if source.len() < required {
            return Err(corrupt(
                "read_vaapi_nv12",
                "NV12 source plane is shorter than its declared geometry",
            ));
        }
        let capacity = row_bytes
            .checked_mul(rows)
            .ok_or_else(|| corrupt("read_vaapi_nv12", "NV12 visible plane size overflowed"))?;
        let mut output = Vec::with_capacity(capacity);
        for row in 0..rows {
            let start = row
                .checked_mul(source_stride)
                .ok_or_else(|| corrupt("read_vaapi_nv12", "NV12 row offset overflowed"))?;
            output.extend_from_slice(&source[start..start + row_bytes]);
        }
        Ok(output)
    }

    fn copy_to_native<F: CrosVideoFrame>(
        frame: &mut F,
        pixels: &Nv12Pixels,
    ) -> std::result::Result<(), String> {
        let pitches = frame.get_plane_pitch();
        let mapping = frame.map_mut()?;
        let planes = mapping.get();
        if planes.len() != 2 || pitches.len() != 2 {
            return Err("VA-API encoder NV12 destination must contain two planes".to_owned());
        }
        let width = pixels.width as usize;
        let height = pixels.height as usize;
        copy_tight_rows(
            &pixels.luma,
            &mut planes[0].borrow_mut(),
            pitches[0],
            width,
            height,
        )?;
        copy_tight_rows(
            &pixels.chroma,
            &mut planes[1].borrow_mut(),
            pitches[1],
            width,
            height.div_ceil(2),
        )?;
        Ok(())
    }

    fn copy_tight_rows(
        source: &[u8],
        destination: &mut [u8],
        destination_stride: usize,
        row_bytes: usize,
        rows: usize,
    ) -> std::result::Result<(), String> {
        if destination_stride < row_bytes {
            return Err("VA-API encoder destination stride is too small".to_owned());
        }
        let source_size = row_bytes
            .checked_mul(rows)
            .ok_or_else(|| "VA-API encoder source size overflowed".to_owned())?;
        let destination_size = destination_stride
            .checked_mul(rows)
            .ok_or_else(|| "VA-API encoder destination size overflowed".to_owned())?;
        if source.len() != source_size || destination.len() < destination_size {
            return Err("VA-API encoder plane geometry is inconsistent".to_owned());
        }
        for row in 0..rows {
            let source_start = row * row_bytes;
            let destination_start = row * destination_stride;
            destination[destination_start..destination_start + row_bytes]
                .copy_from_slice(&source[source_start..source_start + row_bytes]);
        }
        Ok(())
    }

    fn target_bitrate(format: VideoFormat) -> Result<u64> {
        u64::from(format.width())
            .checked_mul(u64::from(format.height()))
            .and_then(|value| value.checked_mul(30))
            .and_then(|value| value.checked_mul(8))
            .map(|value| value.max(256_000))
            .ok_or_else(|| {
                categorized(
                    "create_vaapi_encoder",
                    ErrorCategory::ResourceExhausted,
                    Recoverability::UserCorrectable,
                    "VA-API target bitrate estimate overflowed",
                )
            })
    }

    fn frame_rate(timebase: superi_core::time::Timebase, duration: u64) -> Result<u32> {
        let denominator = u128::from(timebase.numerator())
            .checked_mul(u128::from(duration))
            .ok_or_else(|| corrupt("send_vaapi_frame", "VA-API frame rate overflowed"))?;
        let numerator = u128::from(timebase.denominator());
        let rounded = numerator
            .checked_add(denominator / 2)
            .ok_or_else(|| corrupt("send_vaapi_frame", "VA-API frame rate overflowed"))?
            / denominator;
        u32::try_from(rounded.max(1)).map_err(|_| {
            corrupt(
                "send_vaapi_frame",
                "VA-API frame rate exceeds the native encoder domain",
            )
        })
    }

    fn h264_is_keyframe(data: &[u8]) -> bool {
        annex_b_units(data).any(|unit| unit.first().is_some_and(|byte| byte & 0x1f == 5))
    }

    fn h264_avcc_configuration(data: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut sps = None;
        let mut pps = None;
        for unit in annex_b_units(data) {
            match unit.first().map(|byte| byte & 0x1f) {
                Some(7) if sps.is_none() => sps = Some(unit),
                Some(8) if pps.is_none() => pps = Some(unit),
                _ => {}
            }
        }
        let (Some(sps), Some(pps)) = (sps, pps) else {
            return Ok(None);
        };
        if sps.len() < 4 {
            return Err(corrupt(
                "read_vaapi_h264_configuration",
                "VA-API H.264 SPS is too short to build an AVC configuration record",
            ));
        }
        let sps_length = u16::try_from(sps.len()).map_err(|_| {
            corrupt(
                "read_vaapi_h264_configuration",
                "VA-API H.264 SPS exceeds the AVC configuration length domain",
            )
        })?;
        let pps_length = u16::try_from(pps.len()).map_err(|_| {
            corrupt(
                "read_vaapi_h264_configuration",
                "VA-API H.264 PPS exceeds the AVC configuration length domain",
            )
        })?;
        let mut output = Vec::new();
        output.extend_from_slice(&[1, sps[1], sps[2], sps[3], 0xff, 0xe1]);
        output.extend_from_slice(&sps_length.to_be_bytes());
        output.extend_from_slice(sps);
        output.push(1);
        output.extend_from_slice(&pps_length.to_be_bytes());
        output.extend_from_slice(pps);
        Ok(Some(output))
    }

    fn annex_b_units(data: &[u8]) -> impl Iterator<Item = &[u8]> {
        let starts = annex_b_starts(data);
        starts
            .into_iter()
            .enumerate()
            .filter_map(move |(index, start)| {
                let end = annex_b_starts(data)
                    .get(index + 1)
                    .copied()
                    .unwrap_or(data.len());
                let prefix = if data
                    .get(start..)
                    .is_some_and(|tail| tail.starts_with(&[0, 0, 1]))
                {
                    3
                } else {
                    4
                };
                data.get(start + prefix..end)
                    .filter(|unit| !unit.is_empty())
            })
    }

    fn annex_b_starts(data: &[u8]) -> Vec<usize> {
        let mut starts = Vec::new();
        let mut index = 0;
        while index + 3 <= data.len() {
            if data[index..].starts_with(&[0, 0, 1]) || data[index..].starts_with(&[0, 0, 0, 1]) {
                starts.push(index);
                index += if data[index..].starts_with(&[0, 0, 0, 1]) {
                    4
                } else {
                    3
                };
            } else {
                index += 1;
            }
        }
        starts
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use superi_core::time::Timebase;

        #[test]
        fn h264_output_identifies_idr_and_builds_avcc_configuration() {
            let data = [
                0, 0, 0, 1, 0x67, 0x64, 0, 31, 0, 0, 1, 0x68, 0xee, 0, 0, 0, 1, 0x65, 0x88,
            ];

            assert!(h264_is_keyframe(&data));
            let configuration = h264_avcc_configuration(&data).unwrap().unwrap();
            assert_eq!(
                configuration,
                [1, 0x64, 0, 31, 0xff, 0xe1, 0, 4, 0x67, 0x64, 0, 31, 1, 0, 2, 0x68, 0xee]
            );
            assert!(super::super::normalize_avc_access_unit(&configuration, &data, true).is_ok());
        }

        #[test]
        fn frame_rate_uses_exact_timebase_and_duration() {
            assert_eq!(
                frame_rate(Timebase::new(1, 30_000).unwrap(), 1_001).unwrap(),
                30
            );
            assert_eq!(frame_rate(Timebase::new(1, 24).unwrap(), 1).unwrap(), 24);
        }

        #[test]
        fn visible_rows_drop_padding_without_changing_samples() {
            let source = [1, 2, 3, 9, 4, 5, 6, 9];
            assert_eq!(
                copy_visible_rows(&source, 4, 3, 2).unwrap(),
                [1, 2, 3, 4, 5, 6]
            );
        }

        #[test]
        fn target_bitrate_is_deterministic_for_frame_geometry() {
            let format = VideoFormat::new(
                640,
                360,
                PixelFormat::Nv12,
                ColorSpace::UNSPECIFIED,
                AlphaMode::Opaque,
            )
            .unwrap();
            assert_eq!(target_bitrate(format).unwrap(), 55_296_000);
        }
    }
}

#[cfg(test)]
mod probe_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn render_nodes_are_filtered_and_sorted_stably() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("superi-vaapi-probe-{nonce}"));
        fs::create_dir_all(&directory).unwrap();
        for name in ["renderD129", "card0", "renderD128", "renderDx"] {
            fs::File::create(directory.join(name)).unwrap();
        }

        let nodes = render_nodes_in(&directory).unwrap();
        assert_eq!(
            nodes,
            vec![directory.join("renderD128"), directory.join("renderD129")]
        );

        fs::remove_dir_all(directory).unwrap();
    }
}
