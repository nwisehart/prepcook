//! Host-neutral parsing of git remote and pull/merge request URLs.

use url::Url;

use crate::types::{GitHostError, ProviderKind, RepoInfo};

/// Splits a git remote (https, `ssh://`, or scp-like `git@host:path`) into
/// its host and repository path, the path trimmed of slashes and `.git`.
pub(crate) fn split_remote(remote_url: &str) -> Result<(String, String), GitHostError> {
    let trimmed = remote_url.trim();

    // scp-like: git@host:group/project.git — no scheme, a colon before the path.
    if !trimmed.contains("://")
        && let Some((user_host, path)) = trimmed.split_once(':')
    {
        let host = user_host.rsplit('@').next().unwrap_or(user_host);
        if !host.is_empty() {
            return Ok((host.to_string(), clean_path(path)));
        }
    }

    let url = Url::parse(trimmed)
        .map_err(|e| GitHostError::Repository(format!("Unparseable remote URL: {e}")))?;
    let host = url
        .host_str()
        .ok_or_else(|| GitHostError::Repository("Remote URL has no host".to_string()))?;
    Ok((host.to_string(), clean_path(url.path())))
}

fn clean_path(path: &str) -> String {
    path.trim_matches('/').trim_end_matches(".git").to_string()
}

/// Splits `owner/name` at its last separator, so a GitLab project under
/// nested groups keeps every group in `owner`.
pub(crate) fn split_owner_name(path: &str) -> Option<(&str, &str)> {
    let (owner, name) = path.rsplit_once('/')?;
    (!owner.is_empty() && !name.is_empty()).then_some((owner, name))
}

/// Parses a pull/merge request web URL from any supported host, judged by
/// its shape: `/-/merge_requests/<n>` is GitLab, `/pull/<n>` is GitHub.
pub fn parse_pr_url(pr_url: &str) -> Result<(RepoInfo, i64), GitHostError> {
    if pr_url.contains("/-/merge_requests/") {
        crate::gitlab::parse_pr_url(pr_url)
    } else if pr_url.contains("/pull/") {
        crate::github::parse_pr_url(pr_url)
    } else {
        Err(GitHostError::UnsupportedProvider)
    }
}

/// Parses a git remote into [`RepoInfo`] for whichever host it points at.
pub fn repo_info(remote_url: &str) -> Result<RepoInfo, GitHostError> {
    match crate::detection::detect_provider_from_url(remote_url) {
        ProviderKind::GitHub => crate::github::parse_remote_url(remote_url),
        ProviderKind::GitLab => crate::gitlab::parse_remote_url(remote_url),
        ProviderKind::AzureDevOps | ProviderKind::Unknown => Err(GitHostError::UnsupportedProvider),
    }
}

/// The number at the end of a pull/merge request URL's path.
pub(crate) fn trailing_number(after: &str, pr_url: &str) -> Result<i64, GitHostError> {
    after
        .split(['/', '#', '?'])
        .next()
        .unwrap_or_default()
        .parse()
        .map_err(|_| GitHostError::UnexpectedOutput(format!("Cannot parse number from: {pr_url}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_every_remote_shape() {
        for (remote, host, path) in [
            (
                "https://github.com/acme/widgets.git",
                "github.com",
                "acme/widgets",
            ),
            (
                "https://github.com/acme/widgets",
                "github.com",
                "acme/widgets",
            ),
            (
                "git@github.com:acme/widgets.git",
                "github.com",
                "acme/widgets",
            ),
            (
                "ssh://git@github.com/acme/widgets.git",
                "github.com",
                "acme/widgets",
            ),
            (
                "https://token@gitlab.example.com/a/b/c.git",
                "gitlab.example.com",
                "a/b/c",
            ),
        ] {
            assert_eq!(
                split_remote(remote).unwrap(),
                (host.to_string(), path.to_string()),
                "{remote}"
            );
        }
    }

    #[test]
    fn parse_pr_url_dispatches_on_shape() {
        let (gl, n) = parse_pr_url("https://gitlab.com/g/sub/p/-/merge_requests/12").unwrap();
        assert_eq!(gl.provider, ProviderKind::GitLab);
        assert_eq!(gl.path(), "g/sub/p");
        assert_eq!(n, 12);
        let (gh, n) = parse_pr_url("https://github.com/acme/widgets/pull/7").unwrap();
        assert_eq!(gh.provider, ProviderKind::GitHub);
        assert_eq!(gh.path(), "acme/widgets");
        assert_eq!(n, 7);
        assert!(parse_pr_url("https://example.com/whatever").is_err());
    }
}
