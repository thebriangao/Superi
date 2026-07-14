use std::path::PathBuf;
use std::process::ExitCode;

use superi_fixture_tool::validate_root;

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    let command = arguments.next();
    let root = arguments
        .next()
        .map_or_else(|| PathBuf::from("test-fixtures"), PathBuf::from);
    if command.as_deref() != Some(std::ffi::OsStr::new("check")) || arguments.next().is_some() {
        eprintln!("usage: superi-fixture-tool check [FIXTURE_ROOT]");
        return ExitCode::from(2);
    }

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
