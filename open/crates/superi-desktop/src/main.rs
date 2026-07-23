#![forbid(unsafe_code)]

use std::env;
use std::process;

use superi_desktop::{run, DesktopLaunchOptions};

const HELP: &str = "\
superi-desktop, native Superi host

USAGE:
  superi-desktop [--width N] [--height N]
  superi-desktop --smoke

--smoke presents one real retained scene through the native wgpu surface and exits.
";

fn main() {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "--help" | "-h"))
    {
        print!("{HELP}");
        return;
    }
    let result = DesktopLaunchOptions::parse(arguments).and_then(run);
    if let Err(error) = result {
        eprintln!("superi-desktop: {error}");
        process::exit(1);
    }
}
