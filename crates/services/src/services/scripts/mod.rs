//! A target repository's `.prepcook/scripts.toml` and the runner that executes
//! it: the manifest, the port allocator, and [`ScriptRunner`].

pub mod manifest;
pub mod ports;
pub mod runner;

pub use manifest::{Manifest, ManifestError, load_manifest};
pub use runner::{
    AppRun, AppRunStatus, RunStep, RunnerSettings, ScriptRunState, ScriptRunner, SlotReservation,
    StandUpOptions, StandUpOutcome, StandUpReport, Step, StepReport,
};
