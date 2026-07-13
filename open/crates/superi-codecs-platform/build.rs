use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=CROS_LIBVA_H_PATH");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }

    let include = libva_include_path();
    require_vvc_api(&include);
    let header = "#include <va/va.h>\n#include <va/va_drm.h>\n#include <va/va_drmcommon.h>\n";
    let bindings = bindgen::Builder::default()
        .header_contents("superi-libva-vvc.h", header)
        .clang_arg(format!("-I{}", include.display()))
        .allowlist_function(
            "va(BeginPicture|CreateBuffer|CreateConfig|CreateContext|CreateSurfaces|DestroyBuffer|DestroyConfig|DestroyContext|DestroySurfaces|EndPicture|ErrorStr|ExportSurfaceHandle|GetConfigAttributes|GetDisplayDRM|Initialize|RenderPicture|SyncSurface|Terminate)",
        )
        .allowlist_type("VA.*")
        .allowlist_var("VA.*")
        .constified_enum_module("VA.*")
        .derive_default(true)
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("libva VVC bindings could not be generated");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    bindings
        .write_to_file(output.join("libva_vvc.rs"))
        .expect("libva VVC bindings could not be written");
}

fn libva_include_path() -> PathBuf {
    if let Some(path) = env::var_os("CROS_LIBVA_H_PATH") {
        return PathBuf::from(path);
    }
    let output = Command::new("pkg-config")
        .args(["--cflags-only-I", "libva"])
        .output()
        .expect("pkg-config is required to locate libva headers");
    assert!(
        output.status.success(),
        "pkg-config could not locate libva headers"
    );
    String::from_utf8(output.stdout)
        .expect("pkg-config returned non-UTF-8 output")
        .split_whitespace()
        .find_map(|value| value.strip_prefix("-I"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/include"))
}

fn require_vvc_api(include: &Path) {
    let version = fs::read_to_string(include.join("va/va_version.h"))
        .expect("libva va_version.h is required");
    let major = macro_value(&version, "VA_MAJOR_VERSION");
    let minor = macro_value(&version, "VA_MINOR_VERSION");
    assert!(
        major > 1 || (major == 1 && minor >= 22),
        "H.266 VA-API requires libva API 1.22 or newer, found {major}.{minor}"
    );
    assert!(
        include.join("va/va_dec_vvc.h").exists(),
        "H.266 VA-API requires va/va_dec_vvc.h"
    );
}

fn macro_value(contents: &str, name: &str) -> u32 {
    contents
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            match (fields.next(), fields.next(), fields.next()) {
                (Some("#define"), Some(found), Some(value)) if found == name => value.parse().ok(),
                _ => None,
            }
        })
        .unwrap_or_else(|| panic!("{name} is missing from va_version.h"))
}
