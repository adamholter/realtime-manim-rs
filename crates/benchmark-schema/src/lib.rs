//! Versioned, machine-readable benchmark receipts.

use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

/// Current benchmark receipt schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// A complete, reproducible benchmark result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkResult {
    /// Receipt schema version.
    pub schema_version: u32,
    /// Stable identifier for the benchmark definition.
    pub benchmark_id: String,
    /// UTC timestamp using RFC 3339 syntax.
    pub recorded_at_utc: String,
    /// Sanitized benchmark environment.
    pub environment: EnvironmentSummary,
    /// Workload rendered or evaluated.
    pub workload: Workload,
    /// Named metric values.
    pub samples: Vec<MetricSample>,
    /// Commands and source state that produced the receipt.
    pub provenance: Provenance,
}

/// Privacy-safe environment data needed to interpret a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentSummary {
    /// Operating-system family.
    pub os: String,
    /// Operating-system version.
    pub os_version: String,
    /// CPU architecture.
    pub architecture: String,
    /// Human-readable processor class, without a device identifier.
    pub processor: String,
    /// Installed memory in bytes.
    pub memory_bytes: u64,
    /// Optional display used for an interactive benchmark.
    pub display: Option<DisplaySummary>,
    /// Deliberate load label such as `controlled-idle` or `chrome-slack-codex`.
    pub background_load_profile: String,
}

/// Display mode relevant to an interactive benchmark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplaySummary {
    /// Logical or physical width used by the benchmark.
    pub width: u32,
    /// Logical or physical height used by the benchmark.
    pub height: u32,
    /// Refresh rate in hertz.
    pub refresh_hz: u32,
    /// Device pixel ratio used for rendering.
    pub device_pixel_ratio_milli: u32,
}

/// The scene and render dimensions for a result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workload {
    /// Stable workload name.
    pub name: String,
    /// Scene or fixture path relative to the repository.
    pub scene: String,
    /// Render width and height.
    pub resolution: [u32; 2],
    /// Measured duration.
    pub duration_seconds: f64,
    /// Free-form, non-sensitive notes.
    pub notes: Vec<String>,
}

/// One aggregate or raw numeric measurement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricSample {
    /// Metric name, for example `frame_time`.
    pub name: String,
    /// Unit, for example `ms`.
    pub unit: String,
    /// Statistic, for example `p95`, `mean`, or `raw`.
    pub statistic: String,
    /// Numeric value in the declared unit.
    pub value: f64,
}

/// Reproduction information for the benchmark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// Git revision, or `uncommitted` before the first commit.
    pub revision: String,
    /// Whether tracked or untracked changes existed.
    pub dirty: bool,
    /// Exact commands used.
    pub commands: Vec<String>,
    /// Paths to traces, screenshots, diffs, or raw samples.
    pub artifacts: Vec<String>,
}

/// Semantic validation error after JSON deserialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// A receipt uses an unsupported schema version.
    UnsupportedSchemaVersion(u32),
    /// A required text field is empty.
    EmptyField(&'static str),
    /// The workload resolution has a zero dimension.
    InvalidResolution,
    /// Duration is zero, negative, or non-finite.
    InvalidDuration,
    /// No metrics were recorded.
    MissingSamples,
    /// A metric value is not finite.
    NonFiniteMetric(String),
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion(version) => {
                write!(formatter, "unsupported benchmark schema version {version}")
            }
            Self::EmptyField(field) => write!(formatter, "required field `{field}` is empty"),
            Self::InvalidResolution => write!(formatter, "workload resolution must be non-zero"),
            Self::InvalidDuration => {
                write!(
                    formatter,
                    "workload duration must be finite and greater than zero"
                )
            }
            Self::MissingSamples => write!(formatter, "at least one metric sample is required"),
            Self::NonFiniteMetric(name) => {
                write!(formatter, "metric `{name}` has a non-finite value")
            }
        }
    }
}

impl Error for ValidationError {}

impl BenchmarkResult {
    /// Validate invariants not expressible through Serde alone.
    ///
    /// # Errors
    ///
    /// Returns a [`ValidationError`] for unsupported versions, empty identifiers,
    /// invalid dimensions/durations, or missing/non-finite samples.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(ValidationError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        require_text("benchmark_id", &self.benchmark_id)?;
        require_text("recorded_at_utc", &self.recorded_at_utc)?;
        require_text("environment.os", &self.environment.os)?;
        require_text("environment.os_version", &self.environment.os_version)?;
        require_text("environment.architecture", &self.environment.architecture)?;
        require_text("environment.processor", &self.environment.processor)?;
        require_text(
            "environment.background_load_profile",
            &self.environment.background_load_profile,
        )?;
        require_text("workload.name", &self.workload.name)?;
        require_text("workload.scene", &self.workload.scene)?;

        if self.workload.resolution.contains(&0) {
            return Err(ValidationError::InvalidResolution);
        }
        if !self.workload.duration_seconds.is_finite() || self.workload.duration_seconds <= 0.0 {
            return Err(ValidationError::InvalidDuration);
        }
        if self.samples.is_empty() {
            return Err(ValidationError::MissingSamples);
        }
        for sample in &self.samples {
            require_text("samples.name", &sample.name)?;
            require_text("samples.unit", &sample.unit)?;
            require_text("samples.statistic", &sample.statistic)?;
            if !sample.value.is_finite() {
                return Err(ValidationError::NonFiniteMetric(sample.name.clone()));
            }
        }
        require_text("provenance.revision", &self.provenance.revision)?;
        Ok(())
    }
}

fn require_text(field: &'static str, value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        Err(ValidationError::EmptyField(field))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::{BenchmarkResult, SCHEMA_VERSION, ValidationError};

    const SMOKE_RECEIPT: &str = include_str!("../../../benchmarks/samples/smoke.json");
    const JSON_SCHEMA: &str =
        include_str!("../../../benchmarks/schema/benchmark-result.schema.json");
    const HEAVY_SCENE_RUNTIME_RECEIPTS: [&str; 4] = [
        include_str!("../../../benchmarks/runtime/2026-08-30-paused-redraw-before.receipt.json"),
        include_str!("../../../benchmarks/runtime/2026-08-30-paused-redraw-after.receipt.json"),
        include_str!("../../../benchmarks/runtime/2026-08-30-compile-native-baseline.receipt.json"),
        include_str!("../../../benchmarks/runtime/2026-09-07-accounting.receipt.json"),
    ];

    #[test]
    fn json_schema_is_valid_json_and_declares_v1() {
        let schema: Value = serde_json::from_str(JSON_SCHEMA).expect("schema must be valid JSON");
        assert_eq!(schema["properties"]["schema_version"]["const"], 1);
        assert_eq!(schema["additionalProperties"], false);
    }

    #[test]
    fn smoke_receipt_round_trips_without_information_loss() {
        let receipt: BenchmarkResult =
            serde_json::from_str(SMOKE_RECEIPT).expect("fixture must deserialize");
        receipt.validate().expect("fixture must validate");

        let encoded = serde_json::to_string_pretty(&receipt).expect("receipt must serialize");
        let decoded: BenchmarkResult =
            serde_json::from_str(&encoded).expect("serialized receipt must deserialize");

        assert_eq!(decoded, receipt);
        assert_eq!(decoded.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn heavy_scene_runtime_receipts_validate() {
        for encoded in HEAVY_SCENE_RUNTIME_RECEIPTS {
            let receipt: BenchmarkResult =
                serde_json::from_str(encoded).expect("runtime receipt must deserialize");
            receipt.validate().expect("runtime receipt must validate");
        }
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let malformed = SMOKE_RECEIPT.replacen(
            "\"schema_version\": 1,",
            "\"schema_version\": 1, \"private_device_id\": \"forbidden\",",
            1,
        );

        let error = serde_json::from_str::<BenchmarkResult>(&malformed)
            .expect_err("unknown fields must not deserialize");
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn empty_metrics_are_rejected() {
        let mut receipt: BenchmarkResult =
            serde_json::from_str(SMOKE_RECEIPT).expect("fixture must deserialize");
        receipt.samples.clear();

        assert_eq!(receipt.validate(), Err(ValidationError::MissingSamples));
    }
}
