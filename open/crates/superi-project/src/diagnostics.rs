//! Deterministic semantic project hashing and ordered component diagnostics.

use std::fmt;

use sha2::{Digest, Sha256};
use superi_audio::serialize::CLIP_MIX_FORMAT_REVISION;
use superi_core::error::Result;
use superi_core::ids::{GraphId, ProjectId, TimelineId};
use superi_core::serialization::STABLE_PRIMITIVE_SCHEMA_REVISION;
use superi_core::settings::{ComponentId, VersionIdentifier};
use superi_graph::serialize::GRAPH_DOCUMENT_FORMAT_REVISION;
use superi_timeline::serialize::TIMELINE_STATE_FORMAT_REVISION;

use crate::document::ProjectSnapshot;
use crate::extensions::ProjectExtensionRecordId;
use crate::persist::{
    PreparedProject, StoredGraphKind, PROJECT_EXTENSION_METADATA_FORMAT_REVISION,
};
use crate::settings::PROJECT_SETTINGS_FORMAT_REVISION;

const PROJECT_HASH_DOMAIN_V1: &[u8] = b"superi.project.semantic-hash.v1";

/// Stable digest algorithm used by semantic project hashing.
pub const PROJECT_HASH_ALGORITHM: &str = "sha256";
/// Current framing revision for semantic project hashing and component diagnostics.
pub const PROJECT_HASH_FORMAT_REVISION: u32 = 1;

/// One immutable SHA-256 project or component digest.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectDigest([u8; 32]);

impl ProjectDigest {
    /// Constructs a digest from its exact bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Consumes the digest and returns its exact bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Debug for ProjectDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ProjectDigest")
            .field(&self.to_string())
            .finish()
    }
}

impl fmt::Display for ProjectDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Canonical byte evidence for one project component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectComponentEvidence {
    byte_length: u64,
    digest: ProjectDigest,
}

impl ProjectComponentEvidence {
    const fn new(byte_length: usize, digest: [u8; 32]) -> Self {
        Self {
            byte_length: byte_length as u64,
            digest: ProjectDigest::from_bytes(digest),
        }
    }

    /// Returns the canonical encoded byte length.
    #[must_use]
    pub const fn byte_length(&self) -> u64 {
        self.byte_length
    }

    /// Returns the SHA-256 digest of the canonical encoded bytes.
    #[must_use]
    pub const fn digest(&self) -> ProjectDigest {
        self.digest
    }
}

/// Durable ownership meaning for one retained project graph.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ProjectGraphScope {
    /// A graph compiled for one editorial timeline root.
    Timeline {
        /// Editorial timeline that owns the compilation.
        root_timeline_id: TimelineId,
    },
    /// A named graph retained independently of an editorial timeline.
    Standalone {
        /// Stable editor-facing standalone graph name.
        name: String,
    },
}

/// One ordered, identified component in a semantic project diagnostics report.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectDiagnosticComponent {
    /// Complete canonical editorial project state, including media and relink evidence.
    Timeline {
        /// Canonical timeline-state codec revision.
        format_revision: u32,
        /// Canonical timeline-state byte evidence.
        evidence: ProjectComponentEvidence,
    },
    /// Durable project settings.
    Settings {
        /// Canonical project-settings codec revision.
        format_revision: u32,
        /// Canonical settings byte evidence.
        evidence: ProjectComponentEvidence,
    },
    /// Authored clip-owned audio intent.
    ClipMix {
        /// Canonical clip-mix codec revision.
        format_revision: u32,
        /// Canonical clip-mix byte evidence.
        evidence: ProjectComponentEvidence,
    },
    /// One opaque extension record in stable compound-identity order.
    Extension {
        /// Stable extension component identity.
        extension_id: ComponentId,
        /// Stable extension-owned record identity.
        record_id: ProjectExtensionRecordId,
        /// Canonical extension metadata codec revision.
        metadata_format_revision: u32,
        /// Canonical extension metadata byte evidence.
        metadata: ProjectComponentEvidence,
        /// Schema identity that interprets the opaque payload.
        payload_schema: VersionIdentifier,
        /// Exact opaque extension payload evidence.
        payload: ProjectComponentEvidence,
    },
    /// One retained graph in stable graph-identity order.
    Graph {
        /// Stable graph identity.
        graph_id: GraphId,
        /// Timeline or named standalone ownership meaning.
        scope: ProjectGraphScope,
        /// Authored graph revision encoded by the canonical graph document.
        graph_revision: u64,
        /// Canonical graph codec revision.
        format_revision: u32,
        /// Canonical graph byte evidence.
        evidence: ProjectComponentEvidence,
    },
}

impl ProjectDiagnosticComponent {
    /// Returns the permanent component family code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Timeline { .. } => "timeline",
            Self::Settings { .. } => "settings",
            Self::ClipMix { .. } => "clip_mix",
            Self::Extension { .. } => "extension",
            Self::Graph { .. } => "graph",
        }
    }
}

/// Complete deterministic semantic identity and component evidence for one project snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectDiagnostics {
    project_id: ProjectId,
    root_timeline_id: TimelineId,
    observed_document_revision: u64,
    stable_primitive_schema_revision: u32,
    content_hash: ProjectDigest,
    components: Box<[ProjectDiagnosticComponent]>,
}

impl ProjectDiagnostics {
    /// Computes canonical diagnostics without depending on database bytes, paths, or save identity.
    pub fn from_snapshot(snapshot: &ProjectSnapshot) -> Result<Self> {
        let prepared = PreparedProject::from_snapshot(snapshot)?;
        let mut components =
            Vec::with_capacity(3 + prepared.extensions().len() + prepared.graphs().len());
        components.push(ProjectDiagnosticComponent::Timeline {
            format_revision: TIMELINE_STATE_FORMAT_REVISION,
            evidence: ProjectComponentEvidence::new(
                prepared.timeline_document().len(),
                prepared.timeline_digest(),
            ),
        });
        components.push(ProjectDiagnosticComponent::Settings {
            format_revision: PROJECT_SETTINGS_FORMAT_REVISION,
            evidence: ProjectComponentEvidence::new(
                prepared.settings_document().len(),
                prepared.settings_digest(),
            ),
        });
        components.push(ProjectDiagnosticComponent::ClipMix {
            format_revision: CLIP_MIX_FORMAT_REVISION,
            evidence: ProjectComponentEvidence::new(
                prepared.audio_document().len(),
                prepared.audio_digest(),
            ),
        });

        for (prepared_extension, record) in prepared
            .extensions()
            .iter()
            .zip(snapshot.extension_records().values())
        {
            debug_assert_eq!(
                prepared_extension.extension_id(),
                record.key().extension_id().as_str()
            );
            debug_assert_eq!(
                prepared_extension.record_id(),
                record.key().record_id().as_str()
            );
            components.push(ProjectDiagnosticComponent::Extension {
                extension_id: record.key().extension_id().clone(),
                record_id: record.key().record_id().clone(),
                metadata_format_revision: PROJECT_EXTENSION_METADATA_FORMAT_REVISION,
                metadata: ProjectComponentEvidence::new(
                    prepared_extension.metadata_document().len(),
                    prepared_extension.metadata_digest(),
                ),
                payload_schema: record.payload_schema().clone(),
                payload: ProjectComponentEvidence::new(
                    prepared_extension.payload().len(),
                    prepared_extension.payload_digest(),
                ),
            });
        }

        for graph in prepared.graphs() {
            let scope = match graph.kind() {
                StoredGraphKind::Timeline => ProjectGraphScope::Timeline {
                    root_timeline_id: graph
                        .root_timeline_id()
                        .expect("prepared timeline graph has an owner"),
                },
                StoredGraphKind::Standalone => ProjectGraphScope::Standalone {
                    name: graph
                        .name()
                        .expect("prepared standalone graph has a name")
                        .to_owned(),
                },
            };
            components.push(ProjectDiagnosticComponent::Graph {
                graph_id: graph.graph_id(),
                scope,
                graph_revision: graph.revision(),
                format_revision: GRAPH_DOCUMENT_FORMAT_REVISION,
                evidence: ProjectComponentEvidence::new(graph.document().len(), graph.digest()),
            });
        }

        let content_hash = hash_project(
            snapshot.project_id(),
            snapshot.root_timeline_id(),
            &components,
        );
        Ok(Self {
            project_id: snapshot.project_id(),
            root_timeline_id: snapshot.root_timeline_id(),
            observed_document_revision: snapshot.revision(),
            stable_primitive_schema_revision: STABLE_PRIMITIVE_SCHEMA_REVISION,
            content_hash,
            components: components.into_boxed_slice(),
        })
    }

    /// Returns the stable project identity.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }

    /// Returns the selected editorial root identity.
    #[must_use]
    pub const fn root_timeline_id(&self) -> TimelineId {
        self.root_timeline_id
    }

    /// Returns the observed outer document revision used only for correlation.
    ///
    /// This value is intentionally excluded from the semantic content hash.
    #[must_use]
    pub const fn observed_document_revision(&self) -> u64 {
        self.observed_document_revision
    }

    /// Returns the stable primitive schema revision framed into the content hash.
    #[must_use]
    pub const fn stable_primitive_schema_revision(&self) -> u32 {
        self.stable_primitive_schema_revision
    }

    /// Returns the permanent digest algorithm code.
    #[must_use]
    pub const fn hash_algorithm(&self) -> &'static str {
        PROJECT_HASH_ALGORITHM
    }

    /// Returns the semantic hash framing revision.
    #[must_use]
    pub const fn hash_format_revision(&self) -> u32 {
        PROJECT_HASH_FORMAT_REVISION
    }

    /// Returns the semantic project content hash.
    #[must_use]
    pub const fn content_hash(&self) -> ProjectDigest {
        self.content_hash
    }

    /// Returns components in canonical family and stable identity order.
    #[must_use]
    pub fn components(&self) -> &[ProjectDiagnosticComponent] {
        &self.components
    }
}

fn hash_project(
    project_id: ProjectId,
    root_timeline_id: TimelineId,
    components: &[ProjectDiagnosticComponent],
) -> ProjectDigest {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, PROJECT_HASH_DOMAIN_V1);
    hash_field(&mut hasher, PROJECT_HASH_ALGORITHM.as_bytes());
    hash_field(&mut hasher, &PROJECT_HASH_FORMAT_REVISION.to_be_bytes());
    hash_field(&mut hasher, &STABLE_PRIMITIVE_SCHEMA_REVISION.to_be_bytes());
    hash_field(&mut hasher, &project_id.to_bytes());
    hash_field(&mut hasher, &root_timeline_id.to_bytes());
    hash_field(&mut hasher, &(components.len() as u64).to_be_bytes());
    for component in components {
        hash_component(&mut hasher, component);
    }
    ProjectDigest::from_bytes(hasher.finalize().into())
}

fn hash_component(hasher: &mut Sha256, component: &ProjectDiagnosticComponent) {
    hash_field(hasher, component.code().as_bytes());
    match component {
        ProjectDiagnosticComponent::Timeline {
            format_revision,
            evidence,
        }
        | ProjectDiagnosticComponent::Settings {
            format_revision,
            evidence,
        }
        | ProjectDiagnosticComponent::ClipMix {
            format_revision,
            evidence,
        } => {
            hash_field(hasher, &format_revision.to_be_bytes());
            hash_evidence(hasher, evidence);
        }
        ProjectDiagnosticComponent::Extension {
            extension_id,
            record_id,
            metadata_format_revision,
            metadata,
            payload_schema,
            payload,
        } => {
            hash_field(hasher, extension_id.as_str().as_bytes());
            hash_field(hasher, record_id.as_str().as_bytes());
            hash_field(hasher, &metadata_format_revision.to_be_bytes());
            hash_evidence(hasher, metadata);
            hash_field(hasher, payload_schema.to_string().as_bytes());
            hash_evidence(hasher, payload);
        }
        ProjectDiagnosticComponent::Graph {
            graph_id,
            scope,
            graph_revision,
            format_revision,
            evidence,
        } => {
            hash_field(hasher, &graph_id.to_bytes());
            match scope {
                ProjectGraphScope::Timeline { root_timeline_id } => {
                    hash_field(hasher, b"timeline");
                    hash_field(hasher, &root_timeline_id.to_bytes());
                }
                ProjectGraphScope::Standalone { name } => {
                    hash_field(hasher, b"standalone");
                    hash_field(hasher, name.as_bytes());
                }
            }
            hash_field(hasher, &graph_revision.to_be_bytes());
            hash_field(hasher, &format_revision.to_be_bytes());
            hash_evidence(hasher, evidence);
        }
    }
}

fn hash_evidence(hasher: &mut Sha256, evidence: &ProjectComponentEvidence) {
    hash_field(hasher, &evidence.byte_length().to_be_bytes());
    hash_field(hasher, evidence.digest().as_bytes());
}

fn hash_field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}
