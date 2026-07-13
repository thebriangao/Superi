use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration as StdDuration;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_media_io::backend::{
    BackendCapabilities, BackendDescriptor, BackendRegistration, BackendRegistry, BackendTier,
    MediaBackend,
};
use superi_media_io::decode::{DecodeOutput, Decoder, DecoderConfig};
use superi_media_io::demux::{
    BackendId, ContainerId, MediaSource, MetadataValue, ProbeConfidence, SourceProbe,
    SourceProbeResult, SourceRequest,
};
use superi_media_io::encode::{Encoder, EncoderConfig};
use superi_media_io::operation::{MediaPriority, OperationContext};
use superi_media_io::read::ReadOutcome;

use crate::convert::{
    encode_hex, frame_from_wire, media_id_text, packet_to_wire, read_outcome_from_wire,
    seek_to_wire, source_from_wire, source_location_to_wire, stream_to_wire, time_from_wire,
    vendor_capabilities, vendor_codec_capabilities, SOURCE_HANDLE_METADATA_KEY,
};
use crate::process::{protocol_error, ProcessClient, VendorPluginConfig};
use crate::protocol::{
    DecoderOutputWire, PluginManifest, ProbeResultWire, ProtocolRequest, ProtocolResponse,
    PROTOCOL_REVISION,
};
use crate::VendorRawFormat;

const VENDOR_BACKEND_PRIORITY: u16 = 500;
const CLOSE_TIMEOUT: StdDuration = StdDuration::from_millis(250);

/// Starts and atomically registers explicit user-installed vendor RAW workers.
///
/// No executable is discovered, downloaded, or registered implicitly. Each worker must complete
/// the revisioned handshake before any registration becomes visible.
pub fn register_vendor_plugins(
    registry: &mut BackendRegistry,
    plugins: &[VendorPluginConfig],
    operation: &OperationContext,
) -> Result<()> {
    let mut registrations = Vec::with_capacity(plugins.len());
    for plugin in plugins {
        registrations.push(VendorBackend::connect(plugin, operation)?);
    }
    ensure_backend_ids_available(registry, &registrations)?;
    for registration in registrations {
        registry.register(registration)?;
    }
    Ok(())
}

struct VendorBackend {
    descriptor: BackendDescriptor,
    formats: BTreeSet<VendorRawFormat>,
    process: Arc<ProcessClient>,
    source_leases: Mutex<BTreeMap<String, Weak<SourceLease>>>,
}

impl VendorBackend {
    fn connect(
        config: &VendorPluginConfig,
        operation: &OperationContext,
    ) -> Result<BackendRegistration> {
        let process = ProcessClient::start(config, operation)?;
        let startup = config.startup_operation(operation)?;
        let response = process.request(
            ProtocolRequest::Handshake {
                protocol_revision: PROTOCOL_REVISION,
            },
            &startup,
        )?;
        let ProtocolResponse::Handshake { manifest } = response else {
            return Err(unexpected_response("handshake_vendor_plugin"));
        };
        let (descriptor, formats) = validate_manifest(manifest)?;
        let capabilities = BackendCapabilities::new(vendor_capabilities(&formats)?)
            .with_codec_capabilities(vendor_codec_capabilities(&formats)?)?;
        BackendRegistration::new(
            Arc::new(Self {
                descriptor,
                formats,
                process,
                source_leases: Mutex::new(BTreeMap::new()),
            }),
            capabilities,
            VENDOR_BACKEND_PRIORITY,
            BackendTier::Primary,
        )
    }

    fn source_lease(&self, handle: &str) -> Result<Arc<SourceLease>> {
        let mut leases = self.source_leases.lock().map_err(|_| {
            Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "vendor source lease registry is poisoned",
            )
            .with_context(ErrorContext::new(
                "superi-codecs-vendor.backend",
                "resolve_source_lease",
            ))
        })?;
        let Some(lease) = leases.get(handle).and_then(Weak::upgrade) else {
            leases.remove(handle);
            return Err(Error::new(
                ErrorCategory::Unavailable,
                Recoverability::UserCorrectable,
                "vendor source is no longer open for decoder creation",
            )
            .with_context(
                ErrorContext::new("superi-codecs-vendor.backend", "resolve_source_lease")
                    .with_field("source_handle", handle),
            ));
        };
        Ok(lease)
    }
}

impl MediaBackend for VendorBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn probe_source(
        &self,
        probe: &SourceProbe<'_>,
        operation: &OperationContext,
    ) -> Result<SourceProbeResult> {
        let response = self.process.request(
            ProtocolRequest::Probe {
                source_name: probe.name().map(str::to_owned),
                extension: probe.extension().map(str::to_owned),
                source_length: probe.source_length(),
                complete: probe.is_complete(),
                prefix_hex: encode_hex(probe.bytes()),
            },
            operation,
        )?;
        let ProtocolResponse::Probe { result } = response else {
            return Err(unexpected_response("probe_source"));
        };
        match result {
            ProbeResultWire::NoMatch => Ok(SourceProbeResult::NoMatch),
            ProbeResultWire::NeedMoreData { minimum_bytes } => {
                SourceProbeResult::need_more_data(minimum_bytes)
            }
            ProbeResultWire::Match { format, confidence } => {
                if !self.formats.contains(&format) {
                    return Err(protocol_error(
                        "probe_source",
                        "vendor plugin matched an undeclared RAW format",
                    ));
                }
                Ok(SourceProbeResult::matched(
                    ContainerId::new(format.code())?,
                    ProbeConfidence::new(confidence)?,
                ))
            }
        }
    }

    fn open_source(
        &self,
        request: &SourceRequest,
        operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>> {
        let response = self.process.request(
            ProtocolRequest::Open {
                media_id: media_id_text(request.media_id()),
                location: source_location_to_wire(request.location())?,
                expected_fingerprint: request.expected_fingerprint().map(str::to_owned),
            },
            operation,
        )?;
        let ProtocolResponse::Open { source } = response else {
            return Err(unexpected_response("open_source"));
        };
        let handle = source.source_handle.clone();
        let (info, handle) = match source_from_wire(request, source, &self.formats) {
            Ok(source) => source,
            Err(error) => {
                close_handle(
                    &self.process,
                    ProtocolRequest::CloseSource {
                        source_handle: handle,
                    },
                );
                return Err(error);
            }
        };
        let lease = Arc::new(SourceLease {
            handle: handle.clone(),
            process: Arc::clone(&self.process),
        });
        let mut leases = self.source_leases.lock().map_err(|_| {
            Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "vendor source lease registry is poisoned",
            )
            .with_context(ErrorContext::new(
                "superi-codecs-vendor.backend",
                "open_source",
            ))
        })?;
        leases.retain(|_, lease| lease.strong_count() > 0);
        leases.insert(handle, Arc::downgrade(&lease));
        Ok(Box::new(VendorMediaSource { info, lease }))
    }

    fn create_decoder(
        &self,
        config: &DecoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Decoder>> {
        let format =
            VendorRawFormat::from_code(config.stream().codec().as_str()).ok_or_else(|| {
                unsupported(
                    "create_decoder",
                    "vendor plugin does not decode the requested codec",
                )
            })?;
        if !self.formats.contains(&format) {
            return Err(unsupported(
                "create_decoder",
                "vendor plugin did not declare the requested RAW format",
            ));
        }
        let source_handle = match config.stream().metadata().get(SOURCE_HANDLE_METADATA_KEY) {
            Some(MetadataValue::Text(handle)) if !handle.trim().is_empty() => handle.clone(),
            _ => {
                return Err(Error::new(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "vendor decoder configuration is missing its open source handle",
                )
                .with_context(ErrorContext::new(
                    "superi-codecs-vendor.backend",
                    "create_decoder",
                )))
            }
        };
        let source_lease = self.source_lease(&source_handle)?;
        let response = self.process.request(
            ProtocolRequest::CreateDecoder {
                source_handle,
                stream: stream_to_wire(config.stream())?,
            },
            operation,
        )?;
        let ProtocolResponse::DecoderCreated { decoder_handle } = response else {
            return Err(unexpected_response("create_decoder"));
        };
        if decoder_handle.trim().is_empty() {
            return Err(protocol_error(
                "create_decoder",
                "vendor plugin returned an empty decoder handle",
            ));
        }
        Ok(Box::new(VendorDecoder {
            config: config.clone(),
            decoder_handle,
            process: Arc::clone(&self.process),
            _source_lease: source_lease,
        }))
    }

    fn create_encoder(
        &self,
        _config: &EncoderConfig,
        _operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>> {
        Err(unsupported(
            "create_encoder",
            "vendor RAW plugins are decode-only",
        ))
    }
}

struct SourceLease {
    handle: String,
    process: Arc<ProcessClient>,
}

impl Drop for SourceLease {
    fn drop(&mut self) {
        close_handle(
            &self.process,
            ProtocolRequest::CloseSource {
                source_handle: self.handle.clone(),
            },
        );
    }
}

struct VendorMediaSource {
    info: superi_media_io::demux::SourceInfo,
    lease: Arc<SourceLease>,
}

impl MediaSource for VendorMediaSource {
    fn info(&self) -> &superi_media_io::demux::SourceInfo {
        &self.info
    }

    fn read_packet(
        &mut self,
        operation: &OperationContext,
    ) -> Result<ReadOutcome<superi_media_io::demux::Packet>> {
        let response = self.lease.process.request(
            ProtocolRequest::ReadPacket {
                source_handle: self.lease.handle.clone(),
            },
            operation,
        )?;
        let ProtocolResponse::ReadPacket { outcome } = response else {
            return Err(unexpected_response("read_packet"));
        };
        read_outcome_from_wire(outcome)
    }

    fn seek(
        &mut self,
        request: superi_media_io::demux::SeekRequest,
        operation: &OperationContext,
    ) -> Result<superi_core::time::RationalTime> {
        let response = self.lease.process.request(
            ProtocolRequest::Seek {
                source_handle: self.lease.handle.clone(),
                request: seek_to_wire(request)?,
            },
            operation,
        )?;
        let ProtocolResponse::Seek { selected } = response else {
            return Err(unexpected_response("seek_source"));
        };
        time_from_wire(selected)
    }
}

struct VendorDecoder {
    config: DecoderConfig,
    decoder_handle: String,
    process: Arc<ProcessClient>,
    _source_lease: Arc<SourceLease>,
}

impl Decoder for VendorDecoder {
    fn config(&self) -> &DecoderConfig {
        &self.config
    }

    fn send_packet(
        &mut self,
        packet: superi_media_io::demux::Packet,
        operation: &OperationContext,
    ) -> Result<()> {
        let response = self.process.request(
            ProtocolRequest::SendPacket {
                decoder_handle: self.decoder_handle.clone(),
                packet: packet_to_wire(&packet)?,
            },
            operation,
        )?;
        expect_ack(response, "send_packet")
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<DecodeOutput> {
        let response = self.process.request(
            ProtocolRequest::ReceiveDecoder {
                decoder_handle: self.decoder_handle.clone(),
            },
            operation,
        )?;
        let ProtocolResponse::DecoderOutput { output } = response else {
            return Err(unexpected_response("receive_decoder"));
        };
        match output {
            DecoderOutputWire::Frame { frame } => Ok(DecodeOutput::Frame(frame_from_wire(*frame)?)),
            DecoderOutputWire::NeedInput => Ok(DecodeOutput::NeedInput),
            DecoderOutputWire::EndOfStream => Ok(DecodeOutput::EndOfStream),
        }
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        let response = self.process.request(
            ProtocolRequest::FlushDecoder {
                decoder_handle: self.decoder_handle.clone(),
            },
            operation,
        )?;
        expect_ack(response, "flush_decoder")
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        let response = self.process.request(
            ProtocolRequest::ResetDecoder {
                decoder_handle: self.decoder_handle.clone(),
            },
            operation,
        )?;
        expect_ack(response, "reset_decoder")
    }
}

impl Drop for VendorDecoder {
    fn drop(&mut self) {
        close_handle(
            &self.process,
            ProtocolRequest::CloseDecoder {
                decoder_handle: self.decoder_handle.clone(),
            },
        );
    }
}

fn validate_manifest(
    manifest: PluginManifest,
) -> Result<(BackendDescriptor, BTreeSet<VendorRawFormat>)> {
    if manifest.protocol_revision != PROTOCOL_REVISION {
        return Err(Error::new(
            ErrorCategory::Unsupported,
            Recoverability::UserCorrectable,
            "vendor plugin protocol revision is incompatible",
        )
        .with_context(
            ErrorContext::new("superi-codecs-vendor.backend", "handshake_vendor_plugin")
                .with_field("required_revision", PROTOCOL_REVISION.to_string())
                .with_field("reported_revision", manifest.protocol_revision.to_string()),
        ));
    }
    if manifest.plugin_version.trim().is_empty() || manifest.sdk_version.trim().is_empty() {
        return Err(protocol_error(
            "handshake_vendor_plugin",
            "vendor plugin manifest versions must not be empty",
        ));
    }
    let formats = manifest.formats.iter().copied().collect::<BTreeSet<_>>();
    if formats.is_empty() || formats.len() != manifest.formats.len() {
        return Err(protocol_error(
            "handshake_vendor_plugin",
            "vendor plugin manifest formats must be nonempty and unique",
        ));
    }
    let descriptor =
        BackendDescriptor::new(BackendId::new(manifest.backend_id)?, manifest.display_name)?;
    Ok((descriptor, formats))
}

fn ensure_backend_ids_available(
    registry: &BackendRegistry,
    registrations: &[BackendRegistration],
) -> Result<()> {
    let mut identifiers = registry
        .registrations()
        .map(|registration| registration.backend().descriptor().id().as_str().to_owned())
        .collect::<BTreeSet<_>>();
    for registration in registrations {
        let identifier = registration.backend().descriptor().id().as_str();
        if !identifiers.insert(identifier.to_owned()) {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "backend identifier is already registered",
            )
            .with_context(
                ErrorContext::new("superi-codecs-vendor.backend", "register_vendor_plugins")
                    .with_field("backend_id", identifier),
            ));
        }
    }
    Ok(())
}

fn expect_ack(response: ProtocolResponse, operation: &'static str) -> Result<()> {
    if response == ProtocolResponse::Ack {
        Ok(())
    } else {
        Err(unexpected_response(operation))
    }
}

fn close_handle(process: &ProcessClient, request: ProtocolRequest) {
    if let Ok(operation) =
        OperationContext::new(MediaPriority::Background).with_timeout(CLOSE_TIMEOUT)
    {
        let _ = process.request(request, &operation);
    }
}

fn unexpected_response(operation: &'static str) -> Error {
    protocol_error(
        operation,
        "vendor plugin returned a response for a different operation",
    )
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-vendor.backend", operation))
}
