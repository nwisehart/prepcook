use rmcp::{ErrorData, model::CallToolResult, tool, tool_router};

use super::McpServer;

#[tool_router(router = context_tools_router, vis = "pub")]
impl McpServer {
    #[tool(
        description = "Return project, issue, workspace, and orchestrator-session metadata for the current MCP context."
    )]
    async fn get_context(&self) -> Result<CallToolResult, ErrorData> {
        let mut context = self.context.clone().expect("VK context should exist");
        // The startup snapshot misses the remote linkage when the remote was
        // slow at launch; re-resolve so the agent is told its real issue and
        // project rather than being sent looking for them.
        if (context.project_id.is_none() || context.issue_id.is_none())
            && let Some(link) = self.remote_link().await
        {
            context.project_id = context.project_id.or(link.project_id);
            context.issue_id = context.issue_id.or(link.issue_id);
            context.organization_id = context.organization_id.or(link.organization_id);
        }
        McpServer::success(&context)
    }
}
