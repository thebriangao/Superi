use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::MediaId;
use superi_core::time::{RationalTime, Timebase};
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, BackendDescriptor, BackendRegistration,
    BackendRegistry, BackendTier, FallbackPolicy, MediaBackend,
};
use superi_media_io::decode::{Decoder, DecoderConfig};
use superi_media_io::demux::{
    BackendId, CodecId, ContainerId, MediaSource, Packet, ProbeConfidence, SourceIdentity,
    SourceInfo, SourceLocation, SourceProbe, SourceProbeLimits, SourceProbeResult, SourceRequest,
    StreamId, StreamInfo, StreamKind,
};
use superi_media_io::encode::{Encoder, EncoderConfig};
use superi_media_io::operation::{MediaPriority, OperationContext};
use superi_media_io::read::ReadOutcome;

fn operation() -> OperationContext {
    OperationContext::new(MediaPriority::Interactive)
}

fn probe_failure(message: &'static str) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
        .with_context(ErrorContext::new("probe-contract", "probe_source"))
}

fn video_stream() -> StreamInfo {
    StreamInfo::new(
        StreamId::new(1),
        StreamKind::Video,
        CodecId::new("av1").unwrap(),
        Timebase::integer(24).unwrap(),
    )
}

struct TestSource {
    info: SourceInfo,
    packets: VecDeque<Packet>,
}

impl MediaSource for TestSource {
    fn info(&self) -> &SourceInfo {
        &self.info
    }

    fn read_packet(&mut self, operation: &OperationContext) -> Result<ReadOutcome<Packet>> {
        operation.check("read_packet")?;
        Ok(match self.packets.pop_front() {
            Some(packet) => ReadOutcome::Complete(packet),
            None => ReadOutcome::EndOfStream,
        })
    }

    fn seek(
        &mut self,
        request: superi_media_io::demux::SeekRequest,
        operation: &OperationContext,
    ) -> Result<RationalTime> {
        operation.check("seek")?;
        Ok(request.target())
    }
}

struct ProbeBackend {
    descriptor: BackendDescriptor,
    marker: Arc<[u8]>,
    minimum_bytes: usize,
    container: ContainerId,
    confidence: ProbeConfidence,
    observed_lengths: Arc<Mutex<Vec<usize>>>,
    fail_probe: bool,
    fail_open: bool,
}

impl ProbeBackend {
    fn new(id: &str, marker: &[u8], minimum_bytes: usize, container: &str, confidence: u8) -> Self {
        Self {
            descriptor: BackendDescriptor::new(
                BackendId::new(id).unwrap(),
                format!("{id} test backend"),
            )
            .unwrap(),
            marker: Arc::from(marker),
            minimum_bytes,
            container: ContainerId::new(container).unwrap(),
            confidence: ProbeConfidence::new(confidence).unwrap(),
            observed_lengths: Arc::new(Mutex::new(Vec::new())),
            fail_probe: false,
            fail_open: false,
        }
    }

    fn failing(id: &str) -> Self {
        let mut backend = Self::new(id, b"FAIL", 4, "broken", 100);
        backend.fail_probe = true;
        backend
    }

    fn open_failing(id: &str) -> Self {
        let mut backend = Self::new(id, b"OPEN", 4, "open", 100);
        backend.fail_open = true;
        backend
    }

    fn observed_lengths(&self) -> Arc<Mutex<Vec<usize>>> {
        Arc::clone(&self.observed_lengths)
    }
}

impl MediaBackend for ProbeBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn probe_source(
        &self,
        probe: &SourceProbe<'_>,
        operation: &OperationContext,
    ) -> Result<SourceProbeResult> {
        operation.check("probe_source")?;
        self.observed_lengths
            .lock()
            .unwrap()
            .push(probe.bytes().len());
        if self.fail_probe {
            return Err(probe_failure("test probe failed"));
        }
        if probe.bytes().len() < self.minimum_bytes && !probe.is_complete() {
            return SourceProbeResult::need_more_data(self.minimum_bytes);
        }
        if probe.bytes().starts_with(&self.marker) {
            Ok(SourceProbeResult::matched(
                self.container.clone(),
                self.confidence,
            ))
        } else {
            Ok(SourceProbeResult::NoMatch)
        }
    }

    fn open_source(
        &self,
        request: &SourceRequest,
        operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>> {
        operation.check("open_source")?;
        if self.fail_open {
            return Err(probe_failure("test open failed"));
        }
        let identity = SourceIdentity::new(
            request.media_id(),
            format!("probe:{}", self.descriptor.id()),
        )?;
        Ok(Box::new(TestSource {
            info: SourceInfo::new(identity, vec![video_stream()])?,
            packets: VecDeque::new(),
        }))
    }

    fn create_decoder(
        &self,
        _config: &DecoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Decoder>> {
        operation.check("create_decoder")?;
        Err(probe_failure("decoder is outside the probe contract"))
    }

    fn create_encoder(
        &self,
        _config: &EncoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>> {
        operation.check("create_encoder")?;
        Err(probe_failure("encoder is outside the probe contract"))
    }
}

fn register_source(
    registry: &mut BackendRegistry,
    backend: Arc<ProbeBackend>,
    priority: u16,
    tier: BackendTier,
) {
    registry
        .register(
            BackendRegistration::new(
                backend,
                BackendCapabilities::new([BackendCapability::Source]),
                priority,
                tier,
            )
            .unwrap(),
        )
        .unwrap();
}

fn memory_request(name: &str, data: &[u8]) -> SourceRequest {
    SourceRequest::new(
        MediaId::from_raw(0x33),
        SourceLocation::Memory {
            name: name.into(),
            data: Arc::from(data),
        },
    )
}

struct TemporaryFile(PathBuf);

impl TemporaryFile {
    fn new(data: &[u8]) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "superi-probe-contract-{}-{unique}.mov",
            std::process::id()
        ));
        fs::write(&path, data).unwrap();
        Self(path)
    }

    fn path(&self) -> PathBuf {
        self.0.clone()
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
fn probe_values_reject_ambiguous_confidence_and_limits() {
    assert!(ProbeConfidence::new(0).is_err());
    assert!(ProbeConfidence::new(101).is_err());
    assert_eq!(ProbeConfidence::new(73).unwrap().value(), 73);
    assert!(SourceProbeLimits::new(0, 8).is_err());
    assert!(SourceProbeLimits::new(9, 8).is_err());
    assert_eq!(SourceProbeLimits::new(2, 8).unwrap().initial_bytes(), 2);
    assert_eq!(SourceProbeLimits::new(2, 8).unwrap().maximum_bytes(), 8);
    assert!(SourceProbeResult::need_more_data(0).is_err());

    let location = SourceLocation::Memory {
        name: "folder/CLIP.MOV".into(),
        data: Arc::from([]),
    };
    assert_eq!(location.name(), Some("CLIP.MOV"));
    assert_eq!(location.extension(), Some("MOV"));
}

#[test]
fn registry_probes_bounded_content_incrementally_then_opens_the_match() {
    let backend = Arc::new(ProbeBackend::new("signature", b"SUPER!", 6, "super", 90));
    let observed = backend.observed_lengths();
    let mut registry = BackendRegistry::new();
    register_source(&mut registry, backend, 10, BackendTier::Primary);

    let request = memory_request("misleading.mov", b"SUPER!payload");
    let selection = registry
        .probe_source(
            request,
            SourceProbeLimits::new(2, 8).unwrap(),
            FallbackPolicy::Disallow,
            &operation(),
        )
        .unwrap();

    assert_eq!(*observed.lock().unwrap(), [2, 6]);
    assert_eq!(selection.bytes_examined(), 6);
    assert_eq!(selection.source_length(), 13);
    assert_eq!(selection.primary().container().as_str(), "super");
    assert_eq!(selection.primary().confidence().value(), 90);
    assert_eq!(
        selection.primary().backend().descriptor().id().as_str(),
        "signature"
    );
    assert!(!selection.fallback_used());
    assert!(selection.fallbacks().is_empty());

    let source = selection.open(&operation()).unwrap();
    assert_eq!(source.info().identity().media_id(), MediaId::from_raw(0x33));
    assert_eq!(source.info().identity().fingerprint(), "probe:signature");
}

#[test]
fn path_sources_use_the_same_bounded_probe_contract() {
    let file = TemporaryFile::new(b"SUPER!payload");
    let backend = Arc::new(ProbeBackend::new("path", b"SUPER!", 6, "super", 90));
    let observed = backend.observed_lengths();
    let mut registry = BackendRegistry::new();
    register_source(&mut registry, backend, 10, BackendTier::Primary);

    let selection = registry
        .probe_source(
            SourceRequest::new(MediaId::from_raw(9), SourceLocation::Path(file.path())),
            SourceProbeLimits::new(3, 6).unwrap(),
            FallbackPolicy::Disallow,
            &operation(),
        )
        .unwrap();
    assert_eq!(*observed.lock().unwrap(), [3, 6]);
    assert_eq!(selection.bytes_examined(), 6);
    assert_eq!(selection.source_length(), 13);
    assert_eq!(selection.primary().container().as_str(), "super");
}

#[test]
fn confidence_priority_identity_and_tier_make_selection_deterministic() {
    let low_confidence = Arc::new(ProbeBackend::new("a-low", b"DATA", 4, "low", 40));
    let tied_later = Arc::new(ProbeBackend::new("z-tied", b"DATA", 4, "tied", 90));
    let tied_first = Arc::new(ProbeBackend::new("a-tied", b"DATA", 4, "tied", 90));
    let fallback = Arc::new(ProbeBackend::new("fallback", b"DATA", 4, "fallback", 100));
    let mut registry = BackendRegistry::new();
    register_source(&mut registry, low_confidence, 100, BackendTier::Primary);
    register_source(&mut registry, tied_later, 10, BackendTier::Primary);
    register_source(&mut registry, tied_first, 10, BackendTier::Primary);
    register_source(&mut registry, fallback, 500, BackendTier::Fallback);

    let selection = registry
        .probe_source(
            memory_request("clip.bin", b"DATA"),
            SourceProbeLimits::new(4, 4).unwrap(),
            FallbackPolicy::AllowRegistered,
            &operation(),
        )
        .unwrap();
    assert_eq!(
        selection.primary().backend().descriptor().id().as_str(),
        "a-tied"
    );
    assert_eq!(
        selection
            .fallbacks()
            .iter()
            .map(|candidate| candidate.backend().descriptor().id().as_str())
            .collect::<Vec<_>>(),
        ["fallback"]
    );

    let lower_priority = Arc::new(ProbeBackend::new("a-low-priority", b"DATA", 4, "data", 80));
    let higher_priority = Arc::new(ProbeBackend::new("z-high-priority", b"DATA", 4, "data", 80));
    let mut priority_registry = BackendRegistry::new();
    register_source(
        &mut priority_registry,
        lower_priority,
        9,
        BackendTier::Primary,
    );
    register_source(
        &mut priority_registry,
        higher_priority,
        10,
        BackendTier::Primary,
    );
    let priority_selection = priority_registry
        .probe_source(
            memory_request("clip.bin", b"DATA"),
            SourceProbeLimits::new(4, 4).unwrap(),
            FallbackPolicy::Disallow,
            &operation(),
        )
        .unwrap();
    assert_eq!(
        priority_selection
            .primary()
            .backend()
            .descriptor()
            .id()
            .as_str(),
        "z-high-priority"
    );

    let mut fallback_only = BackendRegistry::new();
    register_source(
        &mut fallback_only,
        Arc::new(ProbeBackend::new(
            "fallback-only",
            b"DATA",
            4,
            "fallback",
            100,
        )),
        10,
        BackendTier::Fallback,
    );
    assert!(fallback_only
        .probe_source(
            memory_request("clip.bin", b"DATA"),
            SourceProbeLimits::new(4, 4).unwrap(),
            FallbackPolicy::Disallow,
            &operation(),
        )
        .is_err());
    let fallback_selection = fallback_only
        .probe_source(
            memory_request("clip.bin", b"DATA"),
            SourceProbeLimits::new(4, 4).unwrap(),
            FallbackPolicy::AllowRegistered,
            &operation(),
        )
        .unwrap();
    assert!(fallback_selection.fallback_used());
    assert_eq!(
        fallback_selection
            .primary()
            .backend()
            .descriptor()
            .id()
            .as_str(),
        "fallback-only"
    );
}

#[test]
fn probe_hints_are_context_only_and_failures_keep_actionable_context() {
    let backend = Arc::new(ProbeBackend::new("content", b"RIGHT", 5, "content", 80));
    let mut registry = BackendRegistry::new();
    register_source(&mut registry, backend, 1, BackendTier::Primary);

    let unsupported = registry.probe_source(
        memory_request("looks-right.content", b"WRONG"),
        SourceProbeLimits::new(2, 5).unwrap(),
        FallbackPolicy::Disallow,
        &operation(),
    );
    let Err(error) = unsupported else {
        panic!("an extension hint must not override nonmatching bytes")
    };
    assert_eq!(error.category(), ErrorCategory::Unsupported);
    assert_eq!(error.contexts()[0].field("bytes_examined"), Some("5"));
    assert_eq!(error.contexts()[0].field("maximum_bytes"), Some("5"));

    let missing = registry.probe_source(
        SourceRequest::new(
            MediaId::from_raw(8),
            SourceLocation::Path(PathBuf::from("missing/probe-contract.mov")),
        ),
        SourceProbeLimits::new(4, 16).unwrap(),
        FallbackPolicy::Disallow,
        &operation(),
    );
    let Err(error) = missing else {
        panic!("a missing source path must fail probing")
    };
    assert_eq!(error.category(), ErrorCategory::NotFound);

    let mut failing = BackendRegistry::new();
    register_source(
        &mut failing,
        Arc::new(ProbeBackend::failing("broken")),
        1,
        BackendTier::Primary,
    );
    let result = failing.probe_source(
        memory_request("clip.bin", b"FAIL"),
        SourceProbeLimits::new(4, 4).unwrap(),
        FallbackPolicy::Disallow,
        &operation(),
    );
    let Err(error) = result else {
        panic!("a backend probe failure must be reported")
    };
    assert_eq!(error.category(), ErrorCategory::Internal);
    assert_eq!(error.contexts()[1].field("backend_id"), Some("broken"));

    let mut open_failing = BackendRegistry::new();
    register_source(
        &mut open_failing,
        Arc::new(ProbeBackend::open_failing("open-broken")),
        1,
        BackendTier::Primary,
    );
    let selection = open_failing
        .probe_source(
            memory_request("clip.bin", b"OPEN"),
            SourceProbeLimits::new(4, 4).unwrap(),
            FallbackPolicy::Disallow,
            &operation(),
        )
        .unwrap();
    let Err(error) = selection.open(&operation()) else {
        panic!("a selected backend open failure must be reported")
    };
    assert_eq!(error.category(), ErrorCategory::Internal);
    assert_eq!(error.contexts()[1].field("backend_id"), Some("open-broken"));
    assert_eq!(error.contexts()[1].field("container_id"), Some("open"));
}

#[test]
fn probing_and_selected_open_propagate_operation_interruption() {
    let backend = Arc::new(ProbeBackend::new("interruptible", b"DATA", 4, "data", 90));
    let mut registry = BackendRegistry::new();
    register_source(&mut registry, backend, 1, BackendTier::Primary);

    let cancelled = operation();
    cancelled.cancellation_token().cancel();
    let error = match registry.probe_source(
        memory_request("clip.bin", b"DATA"),
        SourceProbeLimits::new(4, 4).unwrap(),
        FallbackPolicy::Disallow,
        &cancelled,
    ) {
        Ok(_) => panic!("a cancelled content probe must not begin"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::Cancelled);

    let active = operation();
    let selection = registry
        .probe_source(
            memory_request("clip.bin", b"DATA"),
            SourceProbeLimits::new(4, 4).unwrap(),
            FallbackPolicy::Disallow,
            &active,
        )
        .unwrap();
    active.cancellation_token().cancel();
    let Err(error) = selection.open(&active) else {
        panic!("a cancelled selected open must not begin")
    };
    assert_eq!(error.category(), ErrorCategory::Cancelled);
}
