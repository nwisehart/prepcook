use db::models::repo::Repo;
use rmcp::{
    ErrorData, handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool,
    tool_router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::McpServer;

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpRepoSummary {
    #[schemars(description = "The unique identifier of the repository")]
    id: String,
    #[schemars(description = "The name of the repository")]
    name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetRepoRequest {
    #[schemars(description = "The ID of the repository to retrieve")]
    repo_id: Uuid,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct RepoDetails {
    #[schemars(description = "The unique identifier of the repository")]
    id: String,
    #[schemars(description = "The name of the repository")]
    name: String,
    #[schemars(description = "The display name of the repository")]
    display_name: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct ListReposResponse {
    repos: Vec<McpRepoSummary>,
    count: usize,
}

#[tool_router(router = repos_tools_router, vis = "pub")]
impl McpServer {
    #[tool(description = "List all repositories.")]
    async fn list_repos(&self) -> Result<CallToolResult, ErrorData> {
        let url = self.url("/api/repos");
        let repos: Vec<Repo> = match self.send_json(self.client.get(&url)).await {
            Ok(rs) => rs,
            Err(e) => return Ok(Self::tool_error(e)),
        };

        let repo_summaries: Vec<McpRepoSummary> = repos
            .into_iter()
            .map(|r| McpRepoSummary {
                id: r.id.to_string(),
                name: r.name,
            })
            .collect();

        let response = ListReposResponse {
            count: repo_summaries.len(),
            repos: repo_summaries,
        };

        McpServer::success(&response)
    }

    #[tool(
        description = "Get detailed information about a repository. Its scripts live in the repository's .prepcook/scripts.toml. Use `list_repos` to find available repo IDs."
    )]
    async fn get_repo(
        &self,
        Parameters(GetRepoRequest { repo_id }): Parameters<GetRepoRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/repos/{}", repo_id));
        let repo: Repo = match self.send_json(self.client.get(&url)).await {
            Ok(r) => r,
            Err(e) => return Ok(Self::tool_error(e)),
        };
        McpServer::success(&RepoDetails {
            id: repo.id.to_string(),
            name: repo.name,
            display_name: repo.display_name,
        })
    }
}
