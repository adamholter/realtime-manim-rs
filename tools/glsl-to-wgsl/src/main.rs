use std::env;
use std::io::{self, Read};

use naga::ShaderStage;
use naga::back::wgsl;
use naga::front::glsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let stage = match env::args().nth(1).as_deref() {
        Some("vertex") => ShaderStage::Vertex,
        Some("fragment") => ShaderStage::Fragment,
        _ => return Err("usage: realtime-manim-glsl-to-wgsl vertex|fragment".to_owned()),
    };
    let mut source = String::new();
    io::stdin()
        .read_to_string(&mut source)
        .map_err(|error| format!("could not read GLSL: {error}"))?;
    let mut frontend = glsl::Frontend::default();
    let module = frontend
        .parse(&glsl::Options::from(stage), &source)
        .map_err(|errors| {
            errors
                .errors
                .iter()
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        })?;
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|error| format!("GLSL validation failed: {error}\n{error:#?}"))?;
    let output = wgsl::write_string(&module, &info, wgsl::WriterFlags::empty())
        .map_err(|error| format!("WGSL emission failed: {error}"))?;
    print!("{output}");
    Ok(())
}
