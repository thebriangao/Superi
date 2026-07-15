use std::env;
use std::fs;
use std::process::ExitCode;

use superi_timeline::otio::{export_otio, import_otio, OtioImportOptions, OtioSchemaTarget};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args_os().skip(1);
    let input = arguments
        .next()
        .ok_or("usage: otio_roundtrip <input.otio> <output.otio>")?;
    let output = arguments
        .next()
        .ok_or("usage: otio_roundtrip <input.otio> <output.otio>")?;
    if arguments.next().is_some() {
        return Err("usage: otio_roundtrip <input.otio> <output.otio>".into());
    }

    let imported = import_otio(&fs::read(input)?, OtioImportOptions::default())?;
    for diagnostic in imported.diagnostics() {
        eprintln!(
            "{} {} {} {}",
            diagnostic.severity().code(),
            diagnostic.code(),
            diagnostic.json_pointer(),
            diagnostic.object_id().unwrap_or("<anonymous>")
        );
    }
    fs::write(
        output,
        export_otio(&imported, OtioSchemaTarget::OtioCore0181)?,
    )?;
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
