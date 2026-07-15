use std::path::PathBuf;
use std::process::ExitCode;

use superi_test_report::build_report_file;

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    let command = arguments.next();
    let input = arguments.next().map(PathBuf::from);
    let output = arguments.next().map(PathBuf::from);
    if command.as_deref() != Some(std::ffi::OsStr::new("build"))
        || input.is_none()
        || output.is_none()
        || arguments.next().is_some()
    {
        eprintln!("usage: superi-test-report build INPUT.json OUTPUT.json");
        return ExitCode::from(2);
    }
    let input = input.expect("validated input argument");
    let output = output.expect("validated output argument");
    match build_report_file(&input, &output) {
        Ok(report) => {
            println!("wrote structured test report to {}", output.display());
            if report.has_blocking_findings() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprint!("{error}");
            ExitCode::from(2)
        }
    }
}
