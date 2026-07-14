//! Frame cache color identity for final and intermediate graph outputs.

use superi_graph::node::GraphColorMetadata;
use superi_image::metadata::ColorPipelineMetadata;

/// Complete color identity stored beside one cached graph result.
///
/// Equality and hashing include preserved source payloads and every ordered
/// transform stage, preventing reuse across appearance-changing differences.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CachedFrameColorMetadata {
    pipeline: ColorPipelineMetadata,
}

impl CachedFrameColorMetadata {
    /// Captures the exact graph output color identity.
    #[must_use]
    pub fn from_graph(graph: &GraphColorMetadata) -> Self {
        Self {
            pipeline: graph.pipeline().clone(),
        }
    }

    /// Returns whether a requested result has identical complete color identity.
    #[must_use]
    pub fn matches(&self, requested: &ColorPipelineMetadata) -> bool {
        self.pipeline == *requested
    }

    /// Returns the complete cached source identity and transform history.
    #[must_use]
    pub const fn pipeline(&self) -> &ColorPipelineMetadata {
        &self.pipeline
    }
}
