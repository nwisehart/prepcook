//! The Claude Code skills this repo ships (`assets/agent-skills/`), given to
//! the agent sessions Prep Cook launches and to nothing else.
//!
//! They are written to `<app data>/agent-skills/.claude/skills/<name>/`, and
//! the Claude executor passes that directory with `--add-dir`: Claude Code
//! loads the `.claude/skills` of an added directory for that session, under
//! the skills' own names. Nothing goes into the user's home, where a skill
//! loads in every Claude Code session on the machine, nor into the worktree,
//! where it could reach `git status` or a commit. The toolkit's `setup.sh` is
//! the one thing that puts skills under the user's own skills directory, and
//! only when the user runs it.
//!
//! The files are refreshed on every session start. The repo is the source of
//! truth, so an edit made in the installed copy is discarded rather than left
//! to diverge silently from what review approved.

use std::path::{Path, PathBuf};

use workspace_utils::assets::AgentSkillAssets;

/// The directory handed to Claude Code with `--add-dir`.
pub fn session_skills_root() -> PathBuf {
    workspace_utils::assets::asset_dir().join("agent-skills")
}

/// Writes every shipped skill under `<root>/.claude/skills/`, rewriting any
/// file whose content differs from the shipped one.
pub fn materialize(root: &Path) -> std::io::Result<()> {
    let skills = root.join(".claude").join("skills");
    for path in AgentSkillAssets::iter() {
        let Some(file) = AgentSkillAssets::get(&path) else {
            continue;
        };
        let target = skills.join(path.as_ref());
        if std::fs::read(&target).is_ok_and(|current| current == file.data.as_ref()) {
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, file.data.as_ref())?;
    }
    Ok(())
}

/// Refreshes the session skills and returns the directory to add, or `None`
/// (logged) when they could not be written.
pub fn refreshed_session_skills_root() -> Option<PathBuf> {
    let root = session_skills_root();
    match materialize(&root) {
        Ok(()) => Some(root),
        Err(error) => {
            tracing::warn!(
                ?error,
                "Agent skills: could not write {}; sessions run without them",
                root.display()
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_skill_is_written_and_a_stale_copy_is_replaced() {
        let root = std::env::temp_dir().join(format!("prepcook-skills-{}", uuid::Uuid::new_v4()));
        materialize(&root).unwrap();
        let skill = root.join(".claude/skills/repro-harness/SKILL.md");
        let shipped = std::fs::read(&skill).unwrap();
        assert!(
            root.join(".claude/skills/evergreen-comments/SKILL.md")
                .is_file()
        );

        std::fs::write(&skill, "edited by hand").unwrap();
        materialize(&root).unwrap();
        assert_eq!(std::fs::read(&skill).unwrap(), shipped);
        let _ = std::fs::remove_dir_all(root);
    }
}
