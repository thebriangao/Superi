use std::path::Path;

use superi_dependency_check::check_workspace;

fn main() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    match check_workspace(&workspace) {
        Ok(report) => println!(
            "dependency direction check passed: {} runtime crates, {} internal edges",
            report.checked_packages, report.checked_internal_edges
        ),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
