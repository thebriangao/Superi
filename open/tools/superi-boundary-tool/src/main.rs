use std::path::PathBuf;
use std::process::ExitCode;

use superi_boundary_tool::scan_open_tree;

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    let command = arguments.next();
    let root = arguments
        .next()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    if command.as_deref() != Some(std::ffi::OsStr::new("check")) || arguments.next().is_some() {
        eprintln!("usage: superi-boundary-tool check [OPEN_TREE_ROOT]");
        return ExitCode::from(2);
    }

    match scan_open_tree(&root) {
        Ok(report) => {
            println!(
                "validated {} files across {} Cargo manifests",
                report.files_scanned(),
                report.manifests_scanned()
            );
            ExitCode::SUCCESS
        }
        Err(violations) => {
            eprint!("{violations}");
            ExitCode::FAILURE
        }
    }
}
