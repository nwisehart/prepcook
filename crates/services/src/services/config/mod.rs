use std::path::PathBuf;

use thiserror::Error;

pub mod editor;
pub mod playbooks;
mod versions;

pub use editor::EditorOpenError;

/// The follow-up an implementing agent gets once its pull/merge request
/// exists. `{pr_number}`, `{pr_url}` and `{open_questions}` are filled by the
/// caller; the rest is host-neutral because this text is not interpolated
/// like the playbooks.
pub const DEFAULT_PR_DESCRIPTION_PROMPT: &str = r#"Pull/merge request #{pr_number} at {pr_url} was just created with a placeholder title and description. Analyze the changes on this branch and set the real title and description now, with the host CLI (`gh` or `glab`, whichever serves this repository's remote) in one command — `gh pr edit {pr_number} --title "..." --body "..."` or `glab mr update {pr_number} --title "..." --description "..."` — following this skill:

# Title

Format (strict): `<Prefix>: <Summary> [<KEY>]`

- **`<Prefix>`** is exactly one of: `New`, `Fix`, `Chg` — chosen from the change's INTENT, not the ticket's issue type. `New` adds a feature or capability; `Fix` repairs broken/incorrect behavior; `Chg` changes/refactors existing behavior that isn't a bug fix.
- **`<Summary>`**: imperative mood, capitalized first word ("Preserve…", "Parameterize…"). Describe the outcome/intent, not the files touched or the diff. No trailing period. Aim for ≤ ~72 chars before the ticket tag.
- End with the primary ticket key in brackets, exactly as the tracker writes it: `[<KEY>]`.

Examples: `Fix: Keep item order when a shared list is renamed [<KEY>]` · `New: Let the dev server take its port from the environment [<KEY>]`

# Description

Descriptions: **start with a link to the primary ticket** (when there is one), then **intent and reviewer context** — problem/requirement, approach, key decisions, what to scrutinize. Not a file list or diff narration. **No test-plan checklists.** Plain language.

**CLI**: the request already exists — update it in place (`--body` for `gh`, `--description` for `glab`); never create a second one.

**Ticket**: find `<KEY>` in the commits or the branch name and link it to the ticket's page on the tracker; never leave a bare key. Primary ticket first; others in Notes if needed.

**Sections** (infer from the primary commit prefix):

- `fix:` → **Problem** / **Solution**
- `new:`, `chg:`, `change:` (non-fix) → **Requirement** / **Approach**
- If unclear or mixed, prefer Problem/Solution when fixing broken behavior.
- Optional **Notes** section for caveats or follow-ups.

# Visual evidence

If `.repro/<KEY>/before.png` and `after.png` exist from the implementation session, publish them with this request — a reviewer should not have to check out the branch to see a visual fix.

Where the host can upload a file and return the markdown that references it (GitLab: `glab api --method POST projects/:id/uploads --form file=@.repro/<KEY>/after.png`), upload each one and either embed the pair under **Solution** as Before / After, or post them as a single immediate follow-up comment (`gh pr comment {pr_number} --body "..."` or `glab mr note {pr_number} --message "..."`) when they would crowd the description. Where it cannot, say in the description that the screenshots are on the card. Caption each image with what it shows and the viewport it was taken at, and repeat any caveat the implementation session recorded about what the capture is *not* (component in isolation vs the live page, for instance). No screenshots on disk means no such section — never invent one.

**Bad**: "Changed `a.py`, `b.py`… increased timeout to 60s in three places."
**Good**: Short Problem/Solution or Requirement/Approach focused on intent and design choices.

# Open questions from the review

{open_questions}

If that block is empty, this request has none — write no such section and do not invent one. If it has entries, reproduce them verbatim under a final **Open questions** heading, exactly as given. They are already posted as threads on the diff; repeating them here is so a reviewer knows the request needs their judgment before they open the diff. Do not answer them, rank them, or reword them."#;

pub const DEFAULT_COMMIT_REMINDER_PROMPT: &str = "There are uncommitted changes. Please stage and commit them now with a descriptive commit message.";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("Validation error: {0}")]
    ValidationError(String),
}

pub type Config = versions::v10::Config;
pub type NotificationConfig = versions::v10::NotificationConfig;
pub type EditorConfig = versions::v10::EditorConfig;
pub type ThemeMode = versions::v10::ThemeMode;
pub type SoundFile = versions::v10::SoundFile;
pub type EditorType = versions::v10::EditorType;
pub type GitHubConfig = versions::v10::GitHubConfig;
pub type UiLanguage = versions::v10::UiLanguage;
pub type ShowcaseState = versions::v10::ShowcaseState;
pub type SendMessageShortcut = versions::v10::SendMessageShortcut;
pub type TrackerConfig = versions::v10::TrackerConfig;
pub type TrackerKind = versions::v10::TrackerKind;
pub type TicketingMode = versions::v10::TicketingMode;
pub type IncidentsConfig = versions::v10::IncidentsConfig;
pub type StatusMapping = versions::v10::StatusMapping;
pub type WorktreePoolConfig = versions::v10::WorktreePoolConfig;
pub type OrchestratorConfig = versions::v10::OrchestratorConfig;
pub type OrchestratorMode = versions::v10::OrchestratorMode;
pub type ProjectAutomationDefaults = versions::v10::ProjectAutomationDefaults;
pub type ProjectTrackerOverride = versions::v10::ProjectTrackerOverride;
pub type ProjectSlackOverride = versions::v10::ProjectSlackOverride;
pub type ProjectNotificationsOverride = versions::v10::ProjectNotificationsOverride;
pub type NotificationTarget = versions::v10::NotificationTarget;
pub type TargetOrigin = versions::v10::TargetOrigin;
pub type ProjectMrOverride = versions::v10::ProjectMrOverride;
pub type RetentionOverride = versions::v10::RetentionOverride;
pub type IntegrationsConfig = versions::v10::IntegrationsConfig;
pub type AutonomyMode = versions::v10::AutonomyMode;
pub type EffectiveProjectSettings = versions::v10::EffectiveProjectSettings;
pub type WatchTarget = versions::v10::WatchTarget;
pub type AppRunsConfig = versions::v10::AppRunsConfig;
pub type ProjectReproductionOverride = versions::v10::ProjectReproductionOverride;
pub type DeployWatch = versions::v10::DeployWatch;
pub type ApprovalsConfig = versions::v10::ApprovalsConfig;
pub type InAppGate = versions::v10::InAppGate;
pub use versions::v10::{
    COMMENT_REPLY_PLAYBOOK_KEY, CURRENT_PLAYBOOK_KEYS, INVESTIGATION_PLAYBOOK_KEY,
    PATTERN_PLAYBOOK_KEY, RCA_ANALYSIS_PLAYBOOK_KEY, REVIEWING_PLAYBOOK_KEY,
};

/// Will always return config, trying old schemas or eventually returning default
pub async fn load_config_from_file(config_path: &PathBuf) -> Config {
    match std::fs::read_to_string(config_path) {
        Ok(raw_config) => Config::from(raw_config),
        Err(_) => {
            tracing::info!("No config file found, creating one");
            Config::default()
        }
    }
}

/// Saves the config to the given path
pub async fn save_config_to_file(
    config: &Config,
    config_path: &PathBuf,
) -> Result<(), ConfigError> {
    let raw_config = serde_json::to_string_pretty(config)?;
    std::fs::write(config_path, raw_config)?;
    Ok(())
}
