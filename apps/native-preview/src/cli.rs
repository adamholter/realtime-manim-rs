use std::path::PathBuf;

pub const USAGE: &str = r#"realtime-manim-native-preview

USAGE:
  realtime-manim-native-preview play [SCENE.json] [OPTIONS]
  realtime-manim-native-preview check [SCENE.json] [OPTIONS]
  realtime-manim-native-preview smoke [SCENE.json] [OPTIONS]
  realtime-manim-native-preview render [SCENE.json] --output FRAME.png [OPTIONS]

Omitting SCENE.json uses the bundled animated demo.

OPTIONS:
  --time SECONDS     Explicit scene time (default: 0 for check/render/smoke)
  --width PIXELS     Render width override
  --height PIXELS    Render height override
  --paused           Start interactive playback paused
  --strict           Fail if any visible scene node is unsupported
  -o, --output PATH  PNG output for the render command
  -h, --help         Show this help

PLAY CONTROLS:
  Space pause/resume · Left/Right seek 0.25s · Home restart · R restart · Esc quit
"#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Play,
    Check,
    Smoke,
    Render,
    Help,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Args {
    pub command: Command,
    pub scene_path: Option<PathBuf>,
    pub output_path: Option<PathBuf>,
    pub time: f32,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub paused: bool,
    pub strict: bool,
}

impl Args {
    pub fn parse<I, S>(values: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut values = values.into_iter().map(Into::into);
        let Some(command) = values.next() else {
            return Ok(Self::help());
        };
        let command = match command.as_str() {
            "play" => Command::Play,
            "check" => Command::Check,
            "smoke" => Command::Smoke,
            "render" => Command::Render,
            "help" | "-h" | "--help" => Command::Help,
            unknown => return Err(format!("Unknown command {unknown:?}.\n\n{USAGE}")),
        };
        if command == Command::Help {
            return Ok(Self::help());
        }

        let mut args = Self {
            command,
            scene_path: None,
            output_path: None,
            time: 0.0,
            width: None,
            height: None,
            paused: false,
            strict: false,
        };
        let values = values.collect::<Vec<_>>();
        let mut index = 0;
        while index < values.len() {
            match values[index].as_str() {
                "--time" => {
                    index += 1;
                    args.time = parse_value(values.get(index), "--time")?;
                    if !args.time.is_finite() || args.time < 0.0 {
                        return Err("--time must be a finite non-negative number.".to_owned());
                    }
                }
                "--width" => {
                    index += 1;
                    args.width = Some(parse_dimension(values.get(index), "--width")?);
                }
                "--height" => {
                    index += 1;
                    args.height = Some(parse_dimension(values.get(index), "--height")?);
                }
                "-o" | "--output" => {
                    index += 1;
                    let path = values
                        .get(index)
                        .ok_or_else(|| "--output requires a path.".to_owned())?;
                    args.output_path = Some(PathBuf::from(path));
                }
                "--paused" => args.paused = true,
                "--strict" => args.strict = true,
                "-h" | "--help" => return Ok(Self::help()),
                option if option.starts_with('-') => {
                    return Err(format!("Unknown option {option:?}.\n\n{USAGE}"));
                }
                path => {
                    if args.scene_path.replace(PathBuf::from(path)).is_some() {
                        return Err("Only one scene JSON path may be supplied.".to_owned());
                    }
                }
            }
            index += 1;
        }

        if args.command == Command::Render && args.output_path.is_none() {
            return Err("render requires --output FRAME.png.".to_owned());
        }
        if args.command != Command::Render && args.output_path.is_some() {
            return Err("--output is only valid with the render command.".to_owned());
        }
        Ok(args)
    }

    fn help() -> Self {
        Self {
            command: Command::Help,
            scene_path: None,
            output_path: None,
            time: 0.0,
            width: None,
            height: None,
            paused: false,
            strict: false,
        }
    }
}

fn parse_value<T: std::str::FromStr>(value: Option<&String>, label: &str) -> Result<T, String> {
    value
        .ok_or_else(|| format!("{label} requires a value."))?
        .parse()
        .map_err(|_| format!("{label} has an invalid value."))
}

fn parse_dimension(value: Option<&String>, label: &str) -> Result<u32, String> {
    let value = parse_value(value, label)?;
    if !(1..=16_384).contains(&value) {
        return Err(format!("{label} must be between 1 and 16384."));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::{Args, Command};

    #[test]
    fn no_arguments_is_noninteractive_help() {
        assert_eq!(
            Args::parse(Vec::<String>::new()).unwrap().command,
            Command::Help
        );
    }

    #[test]
    fn parses_headless_render() {
        let args = Args::parse([
            "render",
            "scene.json",
            "--output",
            "frame.png",
            "--time",
            "1.25",
            "--width",
            "640",
            "--height",
            "360",
            "--strict",
        ])
        .unwrap();
        assert_eq!(args.command, Command::Render);
        assert_eq!(args.time, 1.25);
        assert_eq!(args.width, Some(640));
        assert_eq!(args.height, Some(360));
        assert!(args.strict);
    }

    #[test]
    fn render_requires_output() {
        assert!(Args::parse(["render"]).unwrap_err().contains("--output"));
    }
}
