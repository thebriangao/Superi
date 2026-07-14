use std::path::PathBuf;
use std::process::ExitCode;

use superi_fixture_tool::{
    generate_audio_baseline, generate_color_baseline, generate_media_error_baseline,
    generate_otio_baseline, generate_timing_baseline, generate_video_baseline, validate_root,
};

const USAGE: &str = "usage:\n  superi-fixture-tool check [FIXTURE_ROOT]\n  superi-fixture-tool generate-video <OUTPUT_DIRECTORY>\n  superi-fixture-tool generate-audio <OUTPUT_DIRECTORY>\n  superi-fixture-tool generate-timing <OUTPUT_DIRECTORY>\n  superi-fixture-tool generate-color <OUTPUT_DIRECTORY>\n  superi-fixture-tool generate-media-errors <OUTPUT_DIRECTORY>\n  superi-fixture-tool generate-otio <OUTPUT_DIRECTORY>";

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    match arguments.next().as_deref() {
        Some(command) if command == std::ffi::OsStr::new("check") => {
            let root = arguments
                .next()
                .map_or_else(|| PathBuf::from("test-fixtures"), PathBuf::from);
            if arguments.next().is_some() {
                return usage();
            }
            check(root)
        }
        Some(command) if command == std::ffi::OsStr::new("generate-video") => {
            let Some(output_directory) = arguments.next().map(PathBuf::from) else {
                return usage();
            };
            if arguments.next().is_some() {
                return usage();
            }
            generate_video(output_directory)
        }
        Some(command) if command == std::ffi::OsStr::new("generate-audio") => {
            let Some(output_directory) = arguments.next().map(PathBuf::from) else {
                return usage();
            };
            if arguments.next().is_some() {
                return usage();
            }
            generate_audio(output_directory)
        }
        Some(command) if command == std::ffi::OsStr::new("generate-timing") => {
            let Some(output_directory) = arguments.next().map(PathBuf::from) else {
                return usage();
            };
            if arguments.next().is_some() {
                return usage();
            }
            generate_timing(output_directory)
        }
        Some(command) if command == std::ffi::OsStr::new("generate-color") => {
            let Some(output_directory) = arguments.next().map(PathBuf::from) else {
                return usage();
            };
            if arguments.next().is_some() {
                return usage();
            }
            generate_color(output_directory)
        }
        Some(command) if command == std::ffi::OsStr::new("generate-media-errors") => {
            let Some(output_directory) = arguments.next().map(PathBuf::from) else {
                return usage();
            };
            if arguments.next().is_some() {
                return usage();
            }
            generate_media_errors(output_directory)
        }
        Some(command) if command == std::ffi::OsStr::new("generate-otio") => {
            let Some(output_directory) = arguments.next().map(PathBuf::from) else {
                return usage();
            };
            if arguments.next().is_some() {
                return usage();
            }
            generate_otio(output_directory)
        }
        _ => usage(),
    }
}

fn check(root: PathBuf) -> ExitCode {
    match validate_root(&root) {
        Ok(report) => {
            println!(
                "validated {} fixture versions and {} payloads",
                report.fixture_count(),
                report.payload_count()
            );
            ExitCode::SUCCESS
        }
        Err(errors) => {
            eprint!("{errors}");
            ExitCode::FAILURE
        }
    }
}

fn generate_video(output_directory: PathBuf) -> ExitCode {
    match generate_video_baseline(&output_directory) {
        Ok(report) => {
            println!("generated {} video cases", report.case_count());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to generate {}: {error}", output_directory.display());
            ExitCode::FAILURE
        }
    }
}

fn generate_audio(output_directory: PathBuf) -> ExitCode {
    match generate_audio_baseline(&output_directory) {
        Ok(report) => {
            println!("generated {} audio cases", report.case_count());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to generate {}: {error}", output_directory.display());
            ExitCode::FAILURE
        }
    }
}

fn generate_timing(output_directory: PathBuf) -> ExitCode {
    match generate_timing_baseline(&output_directory) {
        Ok(report) => {
            println!(
                "generated {} timing cases and {} samples",
                report.case_count(),
                report.sample_count()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to generate {}: {error}", output_directory.display());
            ExitCode::FAILURE
        }
    }
}

fn generate_color(output_directory: PathBuf) -> ExitCode {
    match generate_color_baseline(&output_directory) {
        Ok(report) => {
            println!(
                "generated {} color images and {} sequence frames",
                report.image_count(),
                report.sequence_frame_count()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to generate {}: {error}", output_directory.display());
            ExitCode::FAILURE
        }
    }
}

fn generate_media_errors(output_directory: PathBuf) -> ExitCode {
    match generate_media_error_baseline(&output_directory) {
        Ok(report) => {
            println!("generated {} media error cases", report.case_count());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to generate {}: {error}", output_directory.display());
            ExitCode::FAILURE
        }
    }
}

fn generate_otio(output_directory: PathBuf) -> ExitCode {
    match generate_otio_baseline(&output_directory) {
        Ok(report) => {
            println!("generated {} OTIO timelines", report.timeline_count());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to generate {}: {error}", output_directory.display());
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}
