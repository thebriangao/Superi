use std::process::ExitCode;

use superi_api_bindings::{
    canonical_bindings_path, check_path, generate_path, CheckStatus, GenerateStatus,
};

fn main() -> ExitCode {
    match run(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: impl Iterator<Item = String>) -> Result<(), String> {
    let arguments = arguments.collect::<Vec<_>>();
    match arguments.as_slice() {
        [command] if command == "generate" => {
            let path = canonical_bindings_path();
            match generate_path(&path).map_err(|error| error.to_string())? {
                GenerateStatus::Unchanged => println!("bindings are already current"),
                GenerateStatus::Written => println!("generated {}", path.display()),
            }
            Ok(())
        }
        [command] if command == "check" => {
            let path = canonical_bindings_path();
            match check_path(&path).map_err(|error| error.to_string())? {
                CheckStatus::Current => {
                    println!("bindings are current");
                    Ok(())
                }
                CheckStatus::Missing => Err(format!(
                    "{} is missing, run superi-api-bindings generate",
                    path.display()
                )),
                CheckStatus::Stale => Err(format!(
                    "{} is stale, run superi-api-bindings generate",
                    path.display()
                )),
            }
        }
        [command] if command == "help" || command == "--help" || command == "-h" => {
            print_usage();
            Ok(())
        }
        _ => {
            print_usage();
            Err("expected exactly one command: generate or check".to_owned())
        }
    }
}

fn print_usage() {
    println!("usage: superi-api-bindings <generate|check>");
}
