//! A target repository's `.prepcook/scripts.toml` and the engine that executes
//! it: the manifest, the port allocator, and [`ScriptRunner`]. Callers stand
//! apps up through a reproduction target ([`crate::services::reproduction`]),
//! which drives the runner.

pub mod manifest;
pub mod ports;
pub mod runner;

pub use manifest::{Manifest, ManifestError, load_manifest};
pub use runner::{
    AppRun, AppRunStatus, RunStatus, RunStep, RunnerSettings, ScriptRunState, ScriptRunner,
    SlotReservation, StandUpOptions, StandUpOutcome, StandUpReport, Step, StepReport,
};
