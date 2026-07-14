//! `superi-cli`, the headless first consumer of the public Superi API.

mod commands;

fn main() {
    std::process::exit(commands::run(std::env::args_os().skip(1)));
}
