//! Render and export color metadata orchestration.

use superi_cache::frame::CachedFrameColorMetadata;
use superi_color::working_space::WorkingSpace;
use superi_core::color_space::ColorSpace;
use superi_core::error::Result;
use superi_graph::node::GraphColorMetadata;
use superi_image::metadata::{
    ColorPipelineMetadata, ColorTransformStage, ColorTransformStageKind, ImageColorTags,
};

/// A viewport branch with one terminal display transform.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ViewportColorMetadata {
    pipeline: ColorPipelineMetadata,
}

impl ViewportColorMetadata {
    /// Creates the standard truthful ACEScg scene to sRGB monitoring branch.
    pub fn standard_srgb_monitoring() -> Result<Self> {
        let scene =
            ColorPipelineMetadata::new(ImageColorTags::new(WorkingSpace::ACESCG.color_space()))?;
        let cached = CachedFrameColorMetadata::from_graph(&GraphColorMetadata::new(scene));
        let display = ColorTransformStage::new(
            ColorTransformStageKind::Display,
            "superi-standard-srgb-monitor",
            ColorSpace::ACESCG,
            ColorSpace::SRGB,
        )?;
        Self::from_cache(&cached, display)
    }

    /// Branches cached scene state into a monitoring pipeline.
    pub fn from_cache(
        cached: &CachedFrameColorMetadata,
        display: ColorTransformStage,
    ) -> Result<Self> {
        require_kind(&display, ColorTransformStageKind::Display)?;
        Ok(Self {
            pipeline: cached.pipeline().clone().with_stage(display)?,
        })
    }

    /// Returns the complete monitoring pipeline.
    #[must_use]
    pub const fn pipeline(&self) -> &ColorPipelineMetadata {
        &self.pipeline
    }
}

/// An export branch with one terminal output transform.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ExportColorMetadata {
    pipeline: ColorPipelineMetadata,
}

impl ExportColorMetadata {
    /// Branches cached scene state into a delivery pipeline.
    pub fn from_cache(
        cached: &CachedFrameColorMetadata,
        output: ColorTransformStage,
    ) -> Result<Self> {
        require_kind(&output, ColorTransformStageKind::Output)?;
        Ok(Self {
            pipeline: cached.pipeline().clone().with_stage(output)?,
        })
    }

    /// Returns the complete delivery pipeline.
    #[must_use]
    pub const fn pipeline(&self) -> &ColorPipelineMetadata {
        &self.pipeline
    }
}

fn require_kind(stage: &ColorTransformStage, expected: ColorTransformStageKind) -> Result<()> {
    if stage.kind() != expected {
        return Err(superi_core::error::Error::new(
            superi_core::error::ErrorCategory::InvalidInput,
            superi_core::error::Recoverability::UserCorrectable,
            "render color branch received the wrong terminal transform kind",
        )
        .with_context(superi_core::error::ErrorContext::new(
            "superi-engine.render",
            "create_color_branch",
        )));
    }
    Ok(())
}
