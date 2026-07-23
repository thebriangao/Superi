#![forbid(unsafe_code)]

//! Native retained interface foundation for Superi.

pub mod capture;
pub mod fixture;
pub mod icons;
pub mod input;
pub mod paint;
pub mod renderer;
pub mod scene;
pub mod semantics;

/// Result type for the retained interface foundation.
pub type Result<T> = std::result::Result<T, UiError>;

/// Classified errors that can be shown without hiding a failed interface path.
#[derive(Debug, thiserror::Error)]
pub enum UiError {
    #[error("invalid interface state: {0}")]
    Invalid(String),
    #[error("interface resource is unavailable: {0}")]
    Unavailable(String),
    #[error("GPU interface operation failed: {0}")]
    Gpu(String),
    #[error("interface I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("interface image encoding failed: {0}")]
    Image(#[from] image::ImageError),
    #[error("interface JSON failed: {0}")]
    Json(#[from] serde_json::Error),
}
