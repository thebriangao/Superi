//! Curated construction seam for the public editor adapter.
//!
//! This module reexports the checked authored-state vocabulary already owned by lower engine
//! dependencies. It adds no wire schema, mutation algorithm, dispatcher, history, or state owner.

pub use superi_audio::mixing::{ChannelMap, ClipMixControls, ClipMixMutation};
pub use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
pub use superi_core::diagnostics::FiniteF64 as DiagnosticFiniteF64;
pub use superi_core::error::{ErrorCategory, ErrorContext, Recoverability};
pub use superi_core::ids::{
    CaptionId, ClipId, EdgeId, GapId, GeneratorId, GraphId, MarkerId, MediaId, MulticamAngleId,
    NodeId, ParameterId, PortId, ProjectId, TimelineId, TrackId, TransitionId,
};
pub use superi_core::pixel::{ChannelLayout, ChannelPosition};
pub use superi_core::settings::{
    CapabilityId, CapabilitySet, ComponentId, SemanticVersion, VersionIdentifier,
};
pub use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
pub use superi_graph::dag::{GraphEdge, GraphEndpoint};
pub use superi_graph::expr::{
    ExpressionVariableName, ParameterAddress, ParameterDriver, ParameterExpression,
    ParameterReference,
};
pub use superi_graph::mutate::{
    EditableNode, EditableParameter, GraphMutation, InstancePort, TypedParameterValue,
};
pub use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchema, NodeSchemaId,
    NodeTypeId, ParameterName, ParameterSchema, PortCardinality, PortName, PortSchema, RoiBehavior,
    TimeBehavior, ValueTypeId,
};
pub use superi_graph::value::{FiniteF64, GraphValue};
pub use superi_project::command_log::{
    ProjectCommandLog, ProjectCommandPayloadDisposition, ProjectCommandRecord,
    ProjectCommandRecordDraft, ProjectCommandRecordKind,
};
pub use superi_project::document::{ProjectDocument, ProjectSnapshot};
pub use superi_project::extensions::{
    ProjectExtensionCommand, ProjectExtensionCommandResult, ProjectExtensionFailure,
    ProjectExtensionKey, ProjectExtensionKind, ProjectExtensionLifecycle, ProjectExtensionRecord,
    ProjectExtensionRecordId,
};
pub use superi_project::media::{
    ProjectMediaCommand, ProjectMediaCommandResult, ProjectMediaImportResult, ReferencedMediaPath,
};
pub use superi_project::{
    negotiate_project_format, project_format_support, ProjectDatabase, ProjectDestinationCollision,
    ProjectFormatIdentity, ProjectFormatRelease, ProjectFormatSupport, ProjectSaveCommand,
    ProjectSaveOperation, ProjectSaveOutcome, ProjectVersionDisposition, ProjectVersionNegotiation,
    ProjectVersionReason,
};
pub use superi_timeline::compile::{
    CompiledTimelineGraphValue, TimelineGraphOrigin, TimelineGraphValue, TrackOutputState,
};
pub use superi_timeline::edit_ops::{
    EditBatchResult, EditKind, EditOperation, EditSide, ExtendMode, RippleSyncAdjustment,
    ThreePointPlacement,
};
pub use superi_timeline::marker_ops::{
    MarkerMutation, MarkerMutationBatchResult, MarkerMutationKind, MarkerMutationOutcome,
};
pub use superi_timeline::markers::{
    Marker, MarkerFlag, MarkerLabel, MarkerNote, MarkerOwner, MetadataKey, MetadataValue,
    TimelineMetadata,
};
pub use superi_timeline::media::{RelinkDecision, RelinkStatus};
pub use superi_timeline::model::{
    AudioChannelRoute, AudioChannelTarget, AudioRouteDestination, AudioRouting,
    AudioTrackSemantics, Caption, CaptionPurpose, CaptionTrackSemantics, Clip, ClipSource,
    DataSchema, DataTrackSemantics, EditorialObjectId, EditorialProject, Gap, Generator,
    LanguageTag, LinkedMediaReference, Timeline, Track, TrackItem, TrackSemantics, Transition,
    VideoCompositing, VideoTrackSemantics,
};
pub use superi_timeline::multicam::{
    MulticamAngle, MulticamAudioPolicy, MulticamClip, MulticamSource, MulticamSwitch,
    MulticamSyncMethod,
};
pub use superi_timeline::retime::{ClipTimeMap, PlaybackRate, RetimeSegment};
pub use superi_timeline::track_ops::{
    TrackCreationKind, TrackMutation, TrackMutationBatchResult, TrackMutationKind,
    TrackMutationOutcome, DEFAULT_TRACK_HEIGHT, MAX_TRACK_HEIGHT, MIN_TRACK_HEIGHT,
};

pub use crate::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult, EngineEvent,
    EngineTransactionId,
};
pub use crate::history::{
    ProjectHistoryActionResult, ProjectHistoryCommand, ProjectHistoryOutcome, ProjectHistoryState,
    ProjectMutation, ProjectMutationKind, RecordedProjectCommand,
};
pub use crate::project_transaction::{
    CompoundProjectAction, CompoundProjectActionResult, CompoundProjectTransaction,
    MAX_COMPOUND_PROJECT_ACTIONS,
};
