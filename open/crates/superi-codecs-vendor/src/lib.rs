//! Host-side adapter for separately installed vendor RAW codec workers.

use serde::{Deserialize, Serialize};

mod backend;
mod convert;
mod process;
pub mod protocol;

pub use backend::register_vendor_plugins;
pub use process::VendorPluginConfig;

/// A proprietary camera RAW family supported only through a separately installed worker.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum VendorRawFormat {
    /// ARRI ARRIRAW or MXF/ARRIRAW media.
    Arriraw,
    /// RED R3D media.
    R3d,
    /// Blackmagic RAW media.
    Braw,
}

impl VendorRawFormat {
    /// Returns the permanent codec and container identifier.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Arriraw => "arriraw",
            Self::R3d => "r3d",
            Self::Braw => "braw",
        }
    }

    /// Resolves a permanent vendor RAW identifier.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "arriraw" => Some(Self::Arriraw),
            "r3d" => Some(Self::R3d),
            "braw" => Some(Self::Braw),
            _ => None,
        }
    }
}

/// Every vendor RAW identity understood by this host protocol.
pub const VENDOR_RAW_FORMATS: [VendorRawFormat; 3] = [
    VendorRawFormat::Arriraw,
    VendorRawFormat::R3d,
    VendorRawFormat::Braw,
];
