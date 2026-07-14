use std::path::PathBuf;
use std::process::ExitCode;

use superi_fixture_tool::{generate_video_baseline, validate_root};

const USAGE: &str = "usage:\n  superi-fixture-tool check [FIXTURE_ROOT]\n  superi-fixture-tool generate-video <OUTPUT_DIRECTORY>";

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

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}
