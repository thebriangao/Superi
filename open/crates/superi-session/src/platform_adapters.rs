//! Read-only current-host adapter declarations behind shared desktop contracts.
//!
//! This owner names adapter families only. Live availability and execution remain with the
//! existing GPU, audio, filesystem, text, monitor, and engine codec owners.

use serde::Serialize;

/// Supported native desktop targets.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopPlatform {
    Macos,
    Windows,
    Linux,
}

/// Platform-specific implementation domains with shared public meaning.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformAdapterDomain {
    Gpu,
    Audio,
    Filesystem,
    Font,
    Monitor,
    Codec,
}

/// Media properties that every adapter path must preserve.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaGuarantee {
    Timing,
    Precision,
    Metadata,
    Alpha,
    PredictableFallback,
}

/// One host implementation behind a stable shared contract identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformAdapterDeclaration {
    domain: PlatformAdapterDomain,
    contract_id: &'static str,
    implementation: &'static str,
}

/// Strict schema-1 declaration for the current desktop host.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopPlatformAdapterSnapshot {
    schema_version: u32,
    platform: DesktopPlatform,
    media_guarantees: [MediaGuarantee; 5],
    adapters: [PlatformAdapterDeclaration; 6],
}

const MEDIA_GUARANTEES: [MediaGuarantee; 5] = [
    MediaGuarantee::Timing,
    MediaGuarantee::Precision,
    MediaGuarantee::Metadata,
    MediaGuarantee::Alpha,
    MediaGuarantee::PredictableFallback,
];

const ADAPTER_CONTRACTS: [(PlatformAdapterDomain, &str); 6] = [
    (PlatformAdapterDomain::Gpu, "superi.adapter.gpu.v1"),
    (PlatformAdapterDomain::Audio, "superi.adapter.audio.v1"),
    (
        PlatformAdapterDomain::Filesystem,
        "superi.adapter.filesystem.v1",
    ),
    (PlatformAdapterDomain::Font, "superi.adapter.font.v1"),
    (PlatformAdapterDomain::Monitor, "superi.adapter.monitor.v1"),
    (PlatformAdapterDomain::Codec, "superi.adapter.codec.v1"),
];

fn snapshot_for(platform: DesktopPlatform) -> DesktopPlatformAdapterSnapshot {
    DesktopPlatformAdapterSnapshot {
        schema_version: 1,
        platform,
        media_guarantees: MEDIA_GUARANTEES,
        adapters: ADAPTER_CONTRACTS.map(|(domain, contract_id)| PlatformAdapterDeclaration {
            domain,
            contract_id,
            implementation: implementation_for(platform, domain),
        }),
    }
}

const fn implementation_for(
    platform: DesktopPlatform,
    domain: PlatformAdapterDomain,
) -> &'static str {
    match (platform, domain) {
        (DesktopPlatform::Macos, PlatformAdapterDomain::Gpu) => "wgpu-metal",
        (DesktopPlatform::Macos, PlatformAdapterDomain::Audio) => "cpal-coreaudio",
        (DesktopPlatform::Macos, PlatformAdapterDomain::Filesystem) => "std-filesystem-macos",
        (DesktopPlatform::Macos, PlatformAdapterDomain::Font) => "swash-inter-4.1",
        (DesktopPlatform::Macos, PlatformAdapterDomain::Monitor) => "winit-cocoa-monitor",
        (DesktopPlatform::Macos, PlatformAdapterDomain::Codec) => "engine-codec-registry-macos",
        (DesktopPlatform::Windows, PlatformAdapterDomain::Gpu) => "wgpu-dx12",
        (DesktopPlatform::Windows, PlatformAdapterDomain::Audio) => "cpal-wasapi",
        (DesktopPlatform::Windows, PlatformAdapterDomain::Filesystem) => "std-filesystem-windows",
        (DesktopPlatform::Windows, PlatformAdapterDomain::Font) => "swash-inter-4.1",
        (DesktopPlatform::Windows, PlatformAdapterDomain::Monitor) => "winit-win32-monitor",
        (DesktopPlatform::Windows, PlatformAdapterDomain::Codec) => "engine-codec-registry-windows",
        (DesktopPlatform::Linux, PlatformAdapterDomain::Gpu) => "wgpu-vulkan",
        (DesktopPlatform::Linux, PlatformAdapterDomain::Audio) => "cpal-alsa",
        (DesktopPlatform::Linux, PlatformAdapterDomain::Filesystem) => "std-filesystem-linux",
        (DesktopPlatform::Linux, PlatformAdapterDomain::Font) => "swash-inter-4.1",
        (DesktopPlatform::Linux, PlatformAdapterDomain::Monitor) => "winit-x11-wayland-monitor",
        (DesktopPlatform::Linux, PlatformAdapterDomain::Codec) => "engine-codec-registry-linux",
    }
}

const fn current_platform() -> DesktopPlatform {
    #[cfg(target_os = "macos")]
    {
        DesktopPlatform::Macos
    }
    #[cfg(target_os = "windows")]
    {
        DesktopPlatform::Windows
    }
    #[cfg(target_os = "linux")]
    {
        DesktopPlatform::Linux
    }
}

/// Returns the exact current-host declarations without probing or mutating hardware.
pub fn current_platform_adapters() -> DesktopPlatformAdapterSnapshot {
    snapshot_for(current_platform())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_target_uses_the_same_ordered_contracts_and_guarantees() {
        let snapshots = [
            snapshot_for(DesktopPlatform::Macos),
            snapshot_for(DesktopPlatform::Windows),
            snapshot_for(DesktopPlatform::Linux),
        ];
        for snapshot in &snapshots {
            assert_eq!(snapshot.schema_version, 1);
            assert_eq!(snapshot.media_guarantees, MEDIA_GUARANTEES);
            assert_eq!(snapshot.adapters.len(), ADAPTER_CONTRACTS.len());
            for (adapter, (domain, contract_id)) in snapshot.adapters.iter().zip(ADAPTER_CONTRACTS)
            {
                assert_eq!(adapter.domain, domain);
                assert_eq!(adapter.contract_id, contract_id);
                assert!(!adapter.implementation.is_empty());
            }
        }
        assert_ne!(snapshots[0].adapters, snapshots[1].adapters);
        assert_ne!(snapshots[1].adapters, snapshots[2].adapters);
    }

    #[test]
    fn command_selects_the_compile_target_without_runtime_guessing() {
        let snapshot = current_platform_adapters();
        assert_eq!(snapshot.platform, current_platform());
    }
}
