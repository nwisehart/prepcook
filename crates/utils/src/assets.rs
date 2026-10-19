use directories::ProjectDirs;
use rust_embed::RustEmbed;

const PROJECT_ROOT: &str = env!("CARGO_MANIFEST_DIR");

pub fn asset_dir() -> std::path::PathBuf {
    let path = if cfg!(debug_assertions) {
        std::path::PathBuf::from(PROJECT_ROOT).join("../../dev_assets")
    } else {
        prod_asset_dir_path()
    };

    // Ensure the directory exists
    if !path.exists() {
        std::fs::create_dir_all(&path).expect("Failed to create asset directory");
    }

    path
    // ✔ macOS → ~/Library/Application Support/MyApp
    // ✔ Linux → ~/.local/share/myapp   (respects XDG_DATA_HOME)
    // ✔ Windows → %APPDATA%\Example\MyApp
}

pub fn prod_asset_dir_path() -> std::path::PathBuf {
    ProjectDirs::from("", "", "prepcook")
        .expect("OS didn't give us a home directory")
        .data_dir()
        .to_path_buf()
}

pub fn config_path() -> std::path::PathBuf {
    asset_dir().join("config.json")
}

/// Snapshots of approved spec/RCA artifacts, keyed by remote issue id.
/// Written at approval time (worktrees are transient); story/subtask
/// snapshots are deleted by the Clean-up automation, epic specs and bug
/// RCAs additionally live in the shared kb.
pub fn approved_specs_dir() -> std::path::PathBuf {
    asset_dir().join("approved-specs")
}

pub fn profiles_path() -> std::path::PathBuf {
    asset_dir().join("profiles.json")
}

#[derive(RustEmbed)]
#[folder = "../../assets/sounds"]
pub struct SoundAssets;

#[derive(RustEmbed)]
#[folder = "../../assets/scripts"]
pub struct ScriptAssets;

/// Claude Code skills the orchestrator ships to every agent session it
/// launches, whatever repository it is working in. Kept here rather than in a
/// product repo for the same reason the stage playbooks are: one place to
/// review a change to how every agent behaves. See `executors::agent_skills`
/// for how a session gets them.
#[derive(RustEmbed)]
#[folder = "../../assets/agent-skills"]
pub struct AgentSkillAssets;
