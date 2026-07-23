#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

use image::ImageFormat;
use superi_ui::capture::{sha256, write_capture, CaptureEnvironment};
use superi_ui::fixture::{FoundationFixture, FoundationState};
use superi_ui::input::{InputEvent, InteractionController, Key};
use superi_ui::scene::NodeId;

const HELP: &str = "\
superi-ui-inspect, private retained-interface controller

USAGE:
  superi-ui-inspect render  --output DIR [--state FILE] [--width N] [--height N] [--scale N]
  superi-ui-inspect inspect [--state FILE] [--width N] [--height N] [--scale N]
  superi-ui-inspect click   --output DIR (--node ID | --x N --y N) [--state FILE]
  superi-ui-inspect key     --output DIR --key KEY [--state FILE]
  superi-ui-inspect type    --output DIR --text TEXT [--state FILE]
  superi-ui-inspect crop    --input PNG --output PNG --rect X,Y,W,H
  superi-ui-inspect compare --left FILE --right FILE

COMMANDS:
  render   render retained foundation pixels and the complete evidence set
  inspect  print semantic JSON from the current retained scene
  click    activate by stable node identity or logical coordinate
  key      dispatch tab, shift-tab, enter, space, escape, left, or right
  type     dispatch logical text input
  crop     crop a captured PNG without changing source pixels
  compare  compare exact artifact bytes and print hashes
";

fn main() {
    if let Err(error) = run() {
        eprintln!("superi-ui-inspect: {error}");
        process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let Some(command) = arguments.first().map(String::as_str) else {
        print!("{HELP}");
        return Ok(());
    };
    if matches!(command, "--help" | "-h" | "help") {
        print!("{HELP}");
        return Ok(());
    }
    let options = parse_options(&arguments[1..])?;
    match command {
        "render" => render_command(&options),
        "inspect" => inspect_command(&options),
        "click" => interaction_command(&options, InteractionKind::Click),
        "key" => interaction_command(&options, InteractionKind::Key),
        "type" => interaction_command(&options, InteractionKind::Text),
        "crop" => crop_command(&options),
        "compare" => compare_command(&options),
        other => Err(format!("unknown command `{other}`\n\n{HELP}")),
    }
}

fn parse_options(arguments: &[String]) -> Result<BTreeMap<String, String>, String> {
    let mut options = BTreeMap::new();
    let mut index = 0;
    while index < arguments.len() {
        let key = arguments[index]
            .strip_prefix("--")
            .ok_or_else(|| format!("expected an option, found `{}`", arguments[index]))?;
        let value = arguments
            .get(index + 1)
            .ok_or_else(|| format!("option `--{key}` requires a value"))?;
        if value.starts_with("--") {
            return Err(format!("option `--{key}` requires a value"));
        }
        if options.insert(key.to_owned(), value.clone()).is_some() {
            return Err(format!("option `--{key}` was provided more than once"));
        }
        index += 2;
    }
    Ok(options)
}

fn render_command(options: &BTreeMap<String, String>) -> Result<(), String> {
    let controller = load_controller(options)?;
    render_controller(options, &controller)
}

fn inspect_command(options: &BTreeMap<String, String>) -> Result<(), String> {
    let controller = load_controller(options)?;
    let fixture = fixture(options)?;
    let scene = fixture
        .scene(controller.state())
        .map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&scene.semantics()).map_err(|error| error.to_string())?
    );
    Ok(())
}

#[derive(Clone, Copy)]
enum InteractionKind {
    Click,
    Key,
    Text,
}

fn interaction_command(
    options: &BTreeMap<String, String>,
    kind: InteractionKind,
) -> Result<(), String> {
    let mut controller = load_controller(options)?;
    let fixture = fixture(options)?;
    let scene = fixture
        .scene(controller.state())
        .map_err(|error| error.to_string())?;
    let event = match kind {
        InteractionKind::Click => {
            if let Some(node) = options.get("node") {
                InputEvent::Activate(NodeId::new(node.clone()).map_err(|error| error.to_string())?)
            } else {
                InputEvent::Pointer {
                    x: parse_required(options, "x")?,
                    y: parse_required(options, "y")?,
                }
            }
        }
        InteractionKind::Key => InputEvent::Key(parse_key(required(options, "key")?)?),
        InteractionKind::Text => InputEvent::Text(required(options, "text")?.to_owned()),
    };
    controller
        .dispatch(&scene, event)
        .map_err(|error| error.to_string())?;
    render_controller(options, &controller)
}

fn render_controller(
    options: &BTreeMap<String, String>,
    controller: &InteractionController,
) -> Result<(), String> {
    let output = PathBuf::from(required(options, "output")?);
    let fixture = fixture(options)?;
    let scene = fixture
        .scene(controller.state())
        .map_err(|error| error.to_string())?;
    let artifacts = write_capture(
        &output,
        &scene,
        controller.transcript(),
        CaptureEnvironment::default(),
    )
    .map_err(|error| error.to_string())?;
    let mut state = serde_json::to_vec_pretty(controller).map_err(|error| error.to_string())?;
    state.push(b'\n');
    fs::write(output.join("state.json"), state).map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&artifacts.hashes).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn crop_command(options: &BTreeMap<String, String>) -> Result<(), String> {
    let input = PathBuf::from(required(options, "input")?);
    let output = PathBuf::from(required(options, "output")?);
    let rectangle = required(options, "rect")?
        .split(',')
        .map(str::parse::<u32>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| "crop rectangle must be X,Y,W,H unsigned integers".to_owned())?;
    if rectangle.len() != 4 || rectangle[2] == 0 || rectangle[3] == 0 {
        return Err("crop rectangle must be X,Y,W,H with positive W and H".to_owned());
    }
    let image = image::open(&input).map_err(|error| error.to_string())?;
    let right = rectangle[0]
        .checked_add(rectangle[2])
        .ok_or_else(|| "crop rectangle is exhausted".to_owned())?;
    let bottom = rectangle[1]
        .checked_add(rectangle[3])
        .ok_or_else(|| "crop rectangle is exhausted".to_owned())?;
    if right > image.width() || bottom > image.height() {
        return Err(format!(
            "crop rectangle exceeds {} by {} input",
            image.width(),
            image.height()
        ));
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    image
        .crop_imm(rectangle[0], rectangle[1], rectangle[2], rectangle[3])
        .save_with_format(&output, ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    println!("{}", output.display());
    Ok(())
}

fn compare_command(options: &BTreeMap<String, String>) -> Result<(), String> {
    let left_path = PathBuf::from(required(options, "left")?);
    let right_path = PathBuf::from(required(options, "right")?);
    let left = fs::read(&left_path).map_err(|error| error.to_string())?;
    let right = fs::read(&right_path).map_err(|error| error.to_string())?;
    let equal = left == right;
    let report = serde_json::json!({
        "equal": equal,
        "left": {
            "path": left_path,
            "bytes": left.len(),
            "sha256": sha256(&left),
        },
        "right": {
            "path": right_path,
            "bytes": right.len(),
            "sha256": sha256(&right),
        }
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
    );
    if equal {
        Ok(())
    } else {
        Err("artifacts differ".to_owned())
    }
}

fn load_controller(options: &BTreeMap<String, String>) -> Result<InteractionController, String> {
    let Some(path) = options.get("state") else {
        return Ok(InteractionController::new(FoundationState::default()));
    };
    let bytes = fs::read(path).map_err(|error| format!("read state `{path}`: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("decode state `{path}`: {error}"))
}

fn fixture(options: &BTreeMap<String, String>) -> Result<FoundationFixture, String> {
    FoundationFixture::new(
        parse_optional(options, "width", 1440_u32)?,
        parse_optional(options, "height", 900_u32)?,
        parse_optional(options, "scale", 1.0_f32)?,
    )
    .map_err(|error| error.to_string())
}

fn required<'a>(options: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str, String> {
    options
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| format!("option `--{name}` is required"))
}

fn parse_required<T>(options: &BTreeMap<String, String>, name: &str) -> Result<T, String>
where
    T: std::str::FromStr,
{
    required(options, name)?
        .parse()
        .map_err(|_| format!("option `--{name}` has an invalid value"))
}

fn parse_optional<T>(
    options: &BTreeMap<String, String>,
    name: &str,
    default: T,
) -> Result<T, String>
where
    T: std::str::FromStr,
{
    match options.get(name) {
        Some(value) => value
            .parse()
            .map_err(|_| format!("option `--{name}` has an invalid value")),
        None => Ok(default),
    }
}

fn parse_key(value: &str) -> Result<Key, String> {
    match value {
        "tab" => Ok(Key::Tab),
        "shift-tab" => Ok(Key::ShiftTab),
        "enter" => Ok(Key::Enter),
        "space" => Ok(Key::Space),
        "escape" => Ok(Key::Escape),
        "left" => Ok(Key::ArrowLeft),
        "right" => Ok(Key::ArrowRight),
        _ => Err(format!("unsupported key `{value}`")),
    }
}
