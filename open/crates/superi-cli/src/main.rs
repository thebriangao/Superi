//! `superi-cli`, headless harness; the public API's first consumer. § 7. Status: skeleton.

mod commands;

fn main() {
    println!(
        "superi {}: scaffold (no engine yet)",
        env!("CARGO_PKG_VERSION")
    );
}
