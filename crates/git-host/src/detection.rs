//! Git hosting provider detection from repository URLs.

use crate::types::ProviderKind;

/// Detect the git hosting provider from a remote URL.
///
/// Supports:
/// - GitHub.com: `https://github.com/owner/repo` or `git@github.com:owner/repo.git`
/// - GitHub Enterprise: URLs containing `github.` (e.g., `https://github.company.com/owner/repo`)
/// - Azure DevOps: `https://dev.azure.com/org/project/_git/repo` or legacy `https://org.visualstudio.com/...`
/// - GitLab: `https://gitlab.com/group/repo`, `git@gitlab.com:group/repo.git`,
///   or self-managed hosts containing `gitlab.` (e.g., `https://gitlab.company.com/...`)
pub(crate) fn detect_provider_from_url(url: &str) -> ProviderKind {
    let url_lower = url.to_lowercase();

    if url_lower.contains("github.com") {
        return ProviderKind::GitHub;
    }

    if url_lower.contains("gitlab.com") {
        return ProviderKind::GitLab;
    }

    // Check Azure patterns before GHE to avoid false positives
    if url_lower.contains("dev.azure.com")
        || url_lower.contains(".visualstudio.com")
        || url_lower.contains("ssh.dev.azure.com")
    {
        return ProviderKind::AzureDevOps;
    }

    // /_git/ is unique to Azure DevOps
    if url_lower.contains("/_git/") {
        return ProviderKind::AzureDevOps;
    }

    // GitHub Enterprise (contains "github." but not the Azure patterns above)
    if url_lower.contains("github.") {
        return ProviderKind::GitHub;
    }

    // Self-managed GitLab (contains "gitlab." but not the Azure patterns
    // above, which could legitimately contain "gitlab." in org/project names)
    if url_lower.contains("gitlab.") {
        return ProviderKind::GitLab;
    }

    ProviderKind::Unknown
}

/// Detect the git hosting provider from a PR URL.
///
/// Supports:
/// - GitHub: `https://github.com/owner/repo/pull/123`
/// - GitHub Enterprise: `https://github.company.com/owner/repo/pull/123`
/// - Azure DevOps: `https://dev.azure.com/org/project/_git/repo/pullrequest/123`
#[cfg(test)]
fn detect_provider_from_pr_url(pr_url: &str) -> ProviderKind {
    let url_lower = pr_url.to_lowercase();

    // GitHub pattern: contains /pull/ in the path
    if url_lower.contains("/pull/") {
        // Could be github.com or GHE
        if url_lower.contains("github.com") || url_lower.contains("github.") {
            return ProviderKind::GitHub;
        }
    }

    // GitLab pattern: contains /-/merge_requests/ in the path
    if url_lower.contains("/-/merge_requests/") {
        return ProviderKind::GitLab;
    }

    // Azure DevOps pattern: contains /pullrequest/ in the path
    if url_lower.contains("/pullrequest/") {
        return ProviderKind::AzureDevOps;
    }

    // Fall back to general URL detection
    detect_provider_from_url(pr_url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_com_https() {
        assert_eq!(
            detect_provider_from_url("https://github.com/owner/repo"),
            ProviderKind::GitHub
        );
        assert_eq!(
            detect_provider_from_url("https://github.com/owner/repo.git"),
            ProviderKind::GitHub
        );
    }

    #[test]
    fn test_github_com_ssh() {
        assert_eq!(
            detect_provider_from_url("git@github.com:owner/repo.git"),
            ProviderKind::GitHub
        );
    }

    #[test]
    fn test_github_enterprise() {
        assert_eq!(
            detect_provider_from_url("https://github.company.com/owner/repo"),
            ProviderKind::GitHub
        );
        assert_eq!(
            detect_provider_from_url("https://github.acme.corp/team/project"),
            ProviderKind::GitHub
        );
        assert_eq!(
            detect_provider_from_url("git@github.internal.io:org/repo.git"),
            ProviderKind::GitHub
        );
    }

    #[test]
    fn test_azure_devops_https() {
        assert_eq!(
            detect_provider_from_url("https://dev.azure.com/org/project/_git/repo"),
            ProviderKind::AzureDevOps
        );
    }

    #[test]
    fn test_azure_devops_ssh() {
        assert_eq!(
            detect_provider_from_url("git@ssh.dev.azure.com:v3/org/project/repo"),
            ProviderKind::AzureDevOps
        );
    }

    #[test]
    fn test_azure_devops_legacy_visualstudio() {
        assert_eq!(
            detect_provider_from_url("https://org.visualstudio.com/project/_git/repo"),
            ProviderKind::AzureDevOps
        );
    }

    #[test]
    fn test_azure_devops_git_path() {
        // Any URL with /_git/ is Azure DevOps
        assert_eq!(
            detect_provider_from_url("https://custom.domain.com/org/project/_git/repo"),
            ProviderKind::AzureDevOps
        );
    }

    #[test]
    fn test_gitlab_com() {
        assert_eq!(
            detect_provider_from_url("https://gitlab.com/owner/repo"),
            ProviderKind::GitLab
        );
        assert_eq!(
            detect_provider_from_url("git@gitlab.com:group/subgroup/repo.git"),
            ProviderKind::GitLab
        );
    }

    #[test]
    fn test_gitlab_self_managed() {
        assert_eq!(
            detect_provider_from_url("https://gitlab.company.com/team/repo"),
            ProviderKind::GitLab
        );
    }

    #[test]
    fn test_gitlab_mr_url() {
        assert_eq!(
            detect_provider_from_pr_url("https://gitlab.com/group/repo/-/merge_requests/42"),
            ProviderKind::GitLab
        );
    }

    #[test]
    fn test_unknown_provider() {
        assert_eq!(
            detect_provider_from_url("https://bitbucket.org/owner/repo"),
            ProviderKind::Unknown
        );
    }

    #[test]
    fn test_pr_url_github() {
        assert_eq!(
            detect_provider_from_pr_url("https://github.com/owner/repo/pull/123"),
            ProviderKind::GitHub
        );
        assert_eq!(
            detect_provider_from_pr_url("https://github.company.com/owner/repo/pull/456"),
            ProviderKind::GitHub
        );
    }

    #[test]
    fn test_pr_url_azure() {
        assert_eq!(
            detect_provider_from_pr_url(
                "https://dev.azure.com/org/project/_git/repo/pullrequest/123"
            ),
            ProviderKind::AzureDevOps
        );
        assert_eq!(
            detect_provider_from_pr_url(
                "https://org.visualstudio.com/project/_git/repo/pullrequest/456"
            ),
            ProviderKind::AzureDevOps
        );
    }
}
