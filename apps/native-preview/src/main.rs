mod cli;
mod geometry;
mod gpu;

use std::fs;
use std::process::ExitCode;

use cli::{Args, Command, USAGE};
use realtime_manim_scene_core::Scene;

const DEMO_SCENE: &str = include_str!("../demo-scene.json");

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("native preview error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse(std::env::args().skip(1))?;
    if args.command == Command::Help {
        print!("{USAGE}");
        return Ok(());
    }

    let source = if let Some(path) = &args.scene_path {
        fs::read_to_string(path)
            .map_err(|error| format!("Could not read {}: {error}", path.display()))?
    } else {
        DEMO_SCENE.to_owned()
    };
    let scene = Scene::from_json(&source)?;

    match args.command {
        Command::Check => check_scene(&scene, &args),
        Command::Smoke => gpu::render_headless(&scene, &args, None),
        Command::Render => gpu::render_headless(&scene, &args, args.output_path.as_deref()),
        Command::Play => gpu::play(scene, &args),
        Command::Help => Ok(()),
    }
}

fn check_scene(scene: &Scene, args: &Args) -> Result<(), String> {
    let frame = scene.evaluate_view(args.time)?;
    let profile = scene.evaluation_profile();
    println!(
        "native check ok scene={:?} time={:.3}s visible_nodes={} total_nodes={} dynamic_nodes={} tracks={} bindings={}",
        scene.title,
        args.time.clamp(0.0, scene.duration),
        frame.nodes.len(),
        profile.node_count,
        profile.dynamic_node_count,
        profile.track_count,
        profile.binding_count,
    );
    Ok(())
}
