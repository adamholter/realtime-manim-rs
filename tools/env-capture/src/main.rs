use std::{error::Error, process::Command};

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
struct EnvironmentManifest {
    capture_version: u32,
    recorded_at_utc: String,
    os: OsSummary,
    hardware: HardwareSummary,
    graphics: Vec<GraphicsSummary>,
    toolchain: ToolchainSummary,
    background_load_profile: String,
    privacy: PrivacySummary,
}

#[derive(Debug, Serialize)]
struct OsSummary {
    family: &'static str,
    version: String,
    architecture: &'static str,
}

#[derive(Debug, Default, Serialize)]
struct HardwareSummary {
    model_name: Option<String>,
    model_identifier: Option<String>,
    chip: Option<String>,
    logical_cpu_count: Option<u32>,
    memory_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
struct GraphicsSummary {
    model: Option<String>,
    core_count: Option<u32>,
    metal_support: Option<String>,
    displays: Vec<DisplaySummary>,
}

#[derive(Debug, Serialize)]
struct DisplaySummary {
    name: Option<String>,
    resolution: Option<String>,
    main: bool,
    online: bool,
}

#[derive(Debug, Serialize)]
struct ToolchainSummary {
    rustc: String,
    cargo: String,
}

#[derive(Debug, Serialize)]
struct PrivacySummary {
    device_identifiers_collected: bool,
    account_identifiers_collected: bool,
    hostnames_collected: bool,
    filesystem_paths_collected: bool,
    process_list_collected: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    let manifest = build_manifest();
    println!("{}", serde_json::to_string_pretty(&manifest)?);
    Ok(())
}

fn build_manifest() -> EnvironmentManifest {
    let profiler = system_profiler();
    EnvironmentManifest {
        capture_version: 1,
        recorded_at_utc: command_text("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"])
            .unwrap_or_else(|| "unknown".to_owned()),
        os: OsSummary {
            family: std::env::consts::OS,
            version: os_version(),
            architecture: std::env::consts::ARCH,
        },
        hardware: hardware_summary(profiler.as_ref()),
        graphics: graphics_summaries(profiler.as_ref()),
        toolchain: ToolchainSummary {
            rustc: command_text("rustc", &["--version"]).unwrap_or_else(|| "unknown".to_owned()),
            cargo: command_text("cargo", &["--version"]).unwrap_or_else(|| "unknown".to_owned()),
        },
        background_load_profile: std::env::var("REALTIME_MANIM_LOAD_PROFILE")
            .unwrap_or_else(|_| "not-recorded".to_owned()),
        privacy: PrivacySummary {
            device_identifiers_collected: false,
            account_identifiers_collected: false,
            hostnames_collected: false,
            filesystem_paths_collected: false,
            process_list_collected: false,
        },
    }
}

fn os_version() -> String {
    command_text("sw_vers", &["-productVersion"])
        .or_else(|| command_text("uname", &["-sr"]))
        .unwrap_or_else(|| "unknown".to_owned())
}

fn system_profiler() -> Option<Value> {
    let output = Command::new("system_profiler")
        .args(["SPHardwareDataType", "SPDisplaysDataType", "-json"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice(&output.stdout).ok()
}

fn hardware_summary(profiler: Option<&Value>) -> HardwareSummary {
    let Some(item) = profiler
        .and_then(|value| value.get("SPHardwareDataType"))
        .and_then(Value::as_array)
        .and_then(|values| values.first())
    else {
        return HardwareSummary::default();
    };

    HardwareSummary {
        model_name: string_field(item, "machine_name"),
        model_identifier: string_field(item, "machine_model"),
        chip: string_field(item, "chip_type"),
        logical_cpu_count: command_text("sysctl", &["-n", "hw.logicalcpu"])
            .and_then(|value| value.parse().ok()),
        memory_bytes: command_text("sysctl", &["-n", "hw.memsize"])
            .and_then(|value| value.parse().ok()),
    }
}

fn graphics_summaries(profiler: Option<&Value>) -> Vec<GraphicsSummary> {
    let Some(items) = profiler
        .and_then(|value| value.get("SPDisplaysDataType"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    items
        .iter()
        .map(|item| {
            let displays = item
                .get("spdisplays_ndrvs")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .map(|display| DisplaySummary {
                            name: string_field(display, "_name"),
                            resolution: string_field(display, "_spdisplays_resolution"),
                            main: string_field(display, "spdisplays_main").as_deref()
                                == Some("spdisplays_yes"),
                            online: string_field(display, "spdisplays_online").as_deref()
                                == Some("spdisplays_yes"),
                        })
                        .collect()
                })
                .unwrap_or_default();

            GraphicsSummary {
                model: string_field(item, "sppci_model"),
                core_count: string_field(item, "sppci_cores").and_then(|value| value.parse().ok()),
                metal_support: string_field(item, "spdisplays_mtlgpufamilysupport"),
                displays,
            }
        })
        .collect()
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(ToOwned::to_owned)
}

fn command_text(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::build_manifest;

    const FORBIDDEN_KEYS: &[&str] = &[
        "serial_number",
        "platform_UUID",
        "provisioning_UDID",
        "hardware_uuid",
        "hostname",
        "username",
        "home",
        "processes",
    ];

    #[test]
    fn manifest_never_contains_private_keys() {
        let value = serde_json::to_value(build_manifest()).expect("manifest must serialize");
        assert_no_forbidden_keys(&value);
    }

    fn assert_no_forbidden_keys(value: &Value) {
        match value {
            Value::Object(object) => {
                for (key, nested) in object {
                    assert!(
                        !FORBIDDEN_KEYS.contains(&key.as_str()),
                        "forbidden key was collected: {key}"
                    );
                    assert_no_forbidden_keys(nested);
                }
            }
            Value::Array(values) => {
                for nested in values {
                    assert_no_forbidden_keys(nested);
                }
            }
            _ => {}
        }
    }
}
