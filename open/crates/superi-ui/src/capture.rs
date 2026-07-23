//! Deterministic private capture artifacts from the foundation retained scene.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::icons::IconRegistry;
use crate::input::InputTranscriptEntry;
use crate::renderer::render_headless;
use crate::scene::Scene;
use crate::Result;

const INTER_FONT: &[u8] = include_bytes!("../assets/InterVariable.ttf");

/// Pinned capture environment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureEnvironment {
    pub fixture: String,
    pub locale: String,
    pub text_direction: String,
    pub clock: String,
    pub random_seed: u64,
    pub renderer_version: String,
    pub font_version: String,
    pub icon_registry_version: String,
}

impl Default for CaptureEnvironment {
    fn default() -> Self {
        Self {
            fixture: "phase-infinity-scaffold-v1".to_owned(),
            locale: "en-US".to_owned(),
            text_direction: "left-to-right".to_owned(),
            clock: "2000-01-01T00:00:00Z".to_owned(),
            random_seed: 0x5355_5045_5249,
            renderer_version: "superi-ui-wgpu-bootstrap-1".to_owned(),
            font_version: "Inter 4.1".to_owned(),
            icon_registry_version: IconRegistry::foundation().version().to_owned(),
        }
    }
}

/// Complete deterministic capture manifest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureManifest {
    pub environment: CaptureEnvironment,
    pub logical_width: u32,
    pub logical_height: u32,
    pub physical_width: u32,
    pub physical_height: u32,
    pub scale_factor: f32,
    pub target_format: String,
    pub adapter: String,
    pub font_sha256: String,
    pub icon_registry_sha256: String,
    pub artifacts: BTreeMap<String, String>,
}

/// Paths and hashes returned after one complete private capture.
#[derive(Clone, Debug)]
pub struct CaptureArtifacts {
    pub png: PathBuf,
    pub semantics: PathBuf,
    pub transcript: PathBuf,
    pub manifest: PathBuf,
    pub hashes: BTreeMap<String, String>,
}

/// Renders one scene through wgpu and emits the complete private evidence set.
pub fn write_capture(
    directory: &Path,
    scene: &Scene,
    transcript: &[InputTranscriptEntry],
    environment: CaptureEnvironment,
) -> Result<CaptureArtifacts> {
    fs::create_dir_all(directory)?;
    let gpu_frame = render_headless(scene)?;
    let frame = gpu_frame.frame();
    let mut png_bytes = Vec::new();
    PngEncoder::new(&mut png_bytes).write_image(
        frame.pixels(),
        frame.width(),
        frame.height(),
        ExtendedColorType::Rgba8,
    )?;
    let semantics_bytes = json_bytes(&scene.semantics())?;
    let transcript_bytes = json_bytes(transcript)?;

    let png = directory.join("surface.png");
    let semantics = directory.join("semantics.json");
    let transcript_path = directory.join("transcript.json");
    let manifest_path = directory.join("manifest.json");
    fs::write(&png, &png_bytes)?;
    fs::write(&semantics, &semantics_bytes)?;
    fs::write(&transcript_path, &transcript_bytes)?;

    let mut hashes = BTreeMap::new();
    hashes.insert("surface.png".to_owned(), sha256(&png_bytes));
    hashes.insert("semantics.json".to_owned(), sha256(&semantics_bytes));
    hashes.insert("transcript.json".to_owned(), sha256(&transcript_bytes));
    let manifest = CaptureManifest {
        environment,
        logical_width: scene.logical_width(),
        logical_height: scene.logical_height(),
        physical_width: frame.width(),
        physical_height: frame.height(),
        scale_factor: scene.scale_factor(),
        target_format: gpu_frame.target_format().to_owned(),
        adapter: gpu_frame.adapter_name().to_owned(),
        font_sha256: sha256(INTER_FONT),
        icon_registry_sha256: IconRegistry::foundation().registry_hash(),
        artifacts: hashes.clone(),
    };
    let manifest_bytes = json_bytes(&manifest)?;
    fs::write(&manifest_path, &manifest_bytes)?;
    hashes.insert("manifest.json".to_owned(), sha256(&manifest_bytes));

    Ok(CaptureArtifacts {
        png,
        semantics,
        transcript: transcript_path,
        manifest: manifest_path,
        hashes,
    })
}

/// Hashes one artifact in lowercase hexadecimal.
#[must_use]
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn json_bytes<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}
