// Import all necessary types from shared types

import {
  ApprovalStatus,
  ApiResponse,
  Config,
  CreateFollowUpAttempt,
  ResetProcessRequest,
  EditorType,
  CreatePrApiRequest,
  CreateTag,
  DirectoryListResponse,
  DirectoryEntry,
  ExecutionProcess,
  ExecutionProcessRepoState,
  GitBranch,
  Repo,
  RepoWithTargetBranch,
  UpdateRepo,
  PlaybookContextResponse,
  RepoScriptsResponse,
  RunScriptRequest,
  RunScriptResponse,
  RunStep,
  ScriptRunState,
  SecretsResponse,
  SetSecretRequest,
  Descriptor,
  IntegrationTestRequest,
  TestOutcome,
  SearchMode,
  SearchResult,
  Tag,
  TagSearchParams,
  UpdateTag,
  UserSystemInfo,
  McpServerQuery,
  UpdateMcpServersBody,
  GetMcpServerResponse,
  AttachmentResponse,
  GitOperationError,
  ApprovalResponse,
  RebaseWorkspaceRequest,
  ChangeTargetBranchRequest,
  ChangeTargetBranchResponse,
  RenameBranchRequest,
  RenameBranchResponse,
  CheckEditorAvailabilityResponse,
  ClaudeSeatStatus,
  ClaudeConnectorsResponse,
  UsageSummary,
  AvailabilityInfo,
  BaseCodingAgent,
  ExecutorConfig,
  DraftFollowUpData,
  AgentPresetOptionsQuery,
  RunAgentSetupRequest,
  RunAgentSetupResponse,
  GhCliSetupError,
  RunScriptError,
  ListOrganizationsResponse,
  OrganizationMemberWithProfile,
  ListMembersResponse,
  OpenEditorResponse,
  OpenEditorRequest,
  PrError,
  Scratch,
  ScratchType,
  CreateScratch,
  UpdateScratch,
  PushError,
  TokenResponse,
  CurrentUserResponse,
  QueueStatus,
  PrCommentsResponse,
  MergeWorkspaceRequest,
  PushWorkspaceRequest,
  RepoBranchStatus,
  AbortConflictsRequest,
  ContinueRebaseRequest,
  Session,
  Workspace,
  StartReviewRequest,
  ReviewError,
  GitRemote,
  ListPrsError,
  PullRequestDetail,
  LinkPrToIssueRequest,
  AttachExistingPrRequest,
  AttachPrResponse,
  CreateWorkspaceFromPrBody,
  CreateWorkspaceFromPrResponse,
  CreateFromPrError,
  CreateAndStartWorkspaceRequest,
  CreateAndStartWorkspaceResponse,
  TrackerImportError,
  TrackerImportRequest,
  TrackerImportResponse,
  TrackerPreviewRequest,
  TrackerPreviewResponse,
} from 'shared/types';
import type { Project as RemoteProject } from 'shared/remote-types';
import type { WorkspaceWithSession } from '@/shared/types/attempt';
import { createWorkspaceWithSession } from '@/shared/types/attempt';
import { makeRequest as makeRemoteRequest } from '@/shared/lib/remoteApi';
import { makeLocalApiRequest } from '@/shared/lib/localApiTransport';

export class ApiError<E = unknown> extends Error {
  public status?: number;
  public error_data?: E;

  constructor(
    message: string,
    public statusCode?: number,
    public response?: Response,
    error_data?: E
  ) {
    super(message);
    this.name = 'ApiError';
    this.status = statusCode;
    this.error_data = error_data;
  }
}

const makeRequest = async (url: string, options: RequestInit = {}) => {
  const headers = new Headers(options.headers ?? {});
  if (!headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }

  return makeLocalApiRequest(url, {
    ...options,
    headers,
  });
};

export type Ok<T> = { success: true; data: T };
export type Err<E> = { success: false; error: E | undefined; message?: string };

// Result type for endpoints that need typed errors
export type Result<T, E> = Ok<T> | Err<E>;

type ListRemoteProjectsResponse = {
  projects: RemoteProject[];
};

// Special handler for Result-returning endpoints
const handleApiResponseAsResult = async <T, E>(
  response: Response
): Promise<Result<T, E>> => {
  if (!response.ok) {
    // HTTP error - no structured error data
    let errorMessage = `Request failed with status ${response.status}`;

    try {
      const errorData = await response.json();
      if (errorData.message) {
        errorMessage = errorData.message;
      }
    } catch {
      errorMessage = response.statusText || errorMessage;
    }

    return {
      success: false,
      error: undefined,
      message: errorMessage,
    };
  }

  const result: ApiResponse<T, E> = await response.json();

  if (!result.success) {
    return {
      success: false,
      error: result.error_data || undefined,
      message: result.message || undefined,
    };
  }

  return { success: true, data: result.data as T };
};

export const handleApiResponse = async <T, E = T>(
  response: Response
): Promise<T> => {
  if (!response.ok) {
    let errorMessage = `Request failed with status ${response.status}`;

    try {
      const errorData = await response.json();
      if (errorData.message) {
        errorMessage = errorData.message;
      }
    } catch {
      // Fallback to status text if JSON parsing fails
      errorMessage = response.statusText || errorMessage;
    }

    console.error('[API Error]', {
      message: errorMessage,
      status: response.status,
      response,
      endpoint: response.url,
      timestamp: new Date().toISOString(),
    });
    throw new ApiError<E>(errorMessage, response.status, response);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  const result: ApiResponse<T, E> = await response.json();

  if (!result.success) {
    // Check for error_data first (structured errors), then fall back to message
    if (result.error_data) {
      console.error('[API Error with data]', {
        error_data: result.error_data,
        message: result.message,
        status: response.status,
        response,
        endpoint: response.url,
        timestamp: new Date().toISOString(),
      });
      // Throw a properly typed error with the error data
      throw new ApiError<E>(
        result.message || 'API request failed',
        response.status,
        response,
        result.error_data
      );
    }

    console.error('[API Error]', {
      message: result.message || 'API request failed',
      status: response.status,
      response,
      endpoint: response.url,
      timestamp: new Date().toISOString(),
    });
    throw new ApiError<E>(
      result.message || 'API request failed',
      response.status,
      response
    );
  }

  return result.data as T;
};

// Knowledge (RCA graph) API — types mirrored by hand from api_types::kb
// (not yet part of generated shared/types.ts).
export interface KbNodeT {
  id: string;
  project_id: string;
  kind: string;
  name: string;
  description: string | null;
  seeded: boolean;
  investigation_jira_key: string | null;
  investigation_edge_count: number | null;
  edge_count: number;
  created_at: string;
}

export interface KbDocumentSummaryT {
  id: string;
  project_id: string;
  kind: string;
  title: string;
  issue_id: string | null;
  jira_key: string | null;
  work_class: string | null;
  summary: string | null;
  tags: string[];
  created_at: string;
  updated_at: string;
}

export interface KbEdgeT {
  id: string;
  project_id: string;
  node_id: string;
  document_id: string;
  rationale: string | null;
  confidence: string | null;
  created_at: string;
}

export const kbApi = {
  listNodes: async (projectId: string): Promise<{ nodes: KbNodeT[] }> => {
    const response = await makeRequest(
      `/api/remote/kb_nodes?project_id=${projectId}`
    );
    return handleApiResponse<{ nodes: KbNodeT[] }>(response);
  },
  listDocuments: async (
    projectId: string,
    kind?: string
  ): Promise<{ documents: KbDocumentSummaryT[] }> => {
    const kindParam = kind ? `&kind=${kind}` : '';
    const response = await makeRequest(
      `/api/remote/kb_documents?project_id=${projectId}${kindParam}`
    );
    return handleApiResponse<{ documents: KbDocumentSummaryT[] }>(response);
  },
  listEdges: async (projectId: string): Promise<{ edges: KbEdgeT[] }> => {
    const response = await makeRequest(
      `/api/remote/kb_edges?project_id=${projectId}`
    );
    return handleApiResponse<{ edges: KbEdgeT[] }>(response);
  },
};

// Sessions API
export const sessionsApi = {
  getByWorkspace: async (workspaceId: string): Promise<Session[]> => {
    const response = await makeRequest(
      `/api/sessions?workspace_id=${workspaceId}`
    );
    return handleApiResponse<Session[]>(response);
  },

  getById: async (sessionId: string): Promise<Session> => {
    const response = await makeRequest(`/api/sessions/${sessionId}`);
    return handleApiResponse<Session>(response);
  },

  create: async (data: {
    workspace_id: string;
    executor?: string;
    name?: string;
  }): Promise<Session> => {
    const response = await makeRequest('/api/sessions', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Session>(response);
  },

  followUp: async (
    sessionId: string,
    data: CreateFollowUpAttempt
  ): Promise<ExecutionProcess> => {
    const response = await makeRequest(`/api/sessions/${sessionId}/follow-up`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<ExecutionProcess>(response);
  },

  startReview: async (
    sessionId: string,
    data: StartReviewRequest
  ): Promise<ExecutionProcess> => {
    const response = await makeRequest(`/api/sessions/${sessionId}/review`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<ExecutionProcess, ReviewError>(response);
  },

  reset: async (
    sessionId: string,
    data: ResetProcessRequest
  ): Promise<void> => {
    const response = await makeRequest(`/api/sessions/${sessionId}/reset`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<void>(response);
  },

  runSetupScript: async (
    sessionId: string
  ): Promise<Result<ExecutionProcess, RunScriptError>> => {
    const response = await makeRequest(`/api/sessions/${sessionId}/setup`, {
      method: 'POST',
    });
    return handleApiResponseAsResult<ExecutionProcess, RunScriptError>(
      response
    );
  },

  update: async (
    sessionId: string,
    data: { name?: string }
  ): Promise<Session> => {
    const response = await makeRequest(`/api/sessions/${sessionId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Session>(response);
  },
};

// Workspace APIs
export const workspacesApi = {
  createAndStart: async (
    data: CreateAndStartWorkspaceRequest
  ): Promise<CreateAndStartWorkspaceResponse> => {
    const response = await makeRequest(`/api/workspaces/start`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<CreateAndStartWorkspaceResponse>(response);
  },

  getAll: async (taskId: string): Promise<Workspace[]> => {
    const response = await makeRequest(`/api/workspaces?task_id=${taskId}`);
    return handleApiResponse<Workspace[]>(response);
  },

  /** Get all workspaces across all tasks (newest first) */
  getAllWorkspaces: async (): Promise<Workspace[]> => {
    const response = await makeRequest('/api/workspaces');
    return handleApiResponse<Workspace[]>(response);
  },

  get: async (workspaceId: string): Promise<Workspace> => {
    const response = await makeRequest(`/api/workspaces/${workspaceId}`);
    return handleApiResponse<Workspace>(response);
  },

  /**
   * Active lavish-axi review session for an artifact in this workspace's
   * worktree (spec/RCA written by a planning agent), or null.
   */
  getLavishSession: async (
    workspaceId: string
  ): Promise<{ url: string; file: string } | null> => {
    const response = await makeRequest(`/api/workspaces/${workspaceId}/lavish`);
    return handleApiResponse<{ url: string; file: string } | null>(response);
  },

  /**
   * Re-opens the lavish review gate for an artifact in the worktree
   * (headless); the session poll picks it up within seconds.
   */
  reopenLavishSession: async (
    workspaceId: string,
    data: { file: string }
  ): Promise<void> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/lavish`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<void>(response);
  },

  update: async (
    workspaceId: string,
    data: { archived?: boolean; pinned?: boolean; name?: string }
  ): Promise<Workspace> => {
    const response = await makeRequest(`/api/workspaces/${workspaceId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Workspace>(response);
  },

  /** Get workspace with latest session */
  getWithSession: async (
    workspaceId: string
  ): Promise<WorkspaceWithSession> => {
    const [workspace, sessions] = await Promise.all([
      workspacesApi.get(workspaceId),
      sessionsApi.getByWorkspace(workspaceId),
    ]);
    return createWorkspaceWithSession(workspace, sessions[0]);
  },

  stop: async (workspaceId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/execution/stop`,
      {
        method: 'POST',
      }
    );
    return handleApiResponse<void>(response);
  },

  delete: async (
    workspaceId: string,
    deleteBranches?: boolean
  ): Promise<void> => {
    const params = new URLSearchParams();
    if (deleteBranches) {
      params.set('delete_branches', 'true');
    }
    const queryString = params.toString();
    const url = `/api/workspaces/${workspaceId}${queryString ? `?${queryString}` : ''}`;
    const response = await makeRequest(url, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  linkToIssue: async (
    workspaceId: string,
    projectId: string,
    issueId: string
  ): Promise<void> => {
    const response = await makeRequest(`/api/workspaces/${workspaceId}/links`, {
      method: 'POST',
      body: JSON.stringify({ project_id: projectId, issue_id: issueId }),
    });
    return handleApiResponse<void>(response);
  },

  unlinkFromIssue: async (workspaceId: string): Promise<void> => {
    const response = await makeRequest(`/api/workspaces/${workspaceId}/links`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  runAgentSetup: async (
    workspaceId: string,
    data: RunAgentSetupRequest
  ): Promise<RunAgentSetupResponse> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/integration/agent/setup`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<RunAgentSetupResponse>(response);
  },

  openEditor: async (
    workspaceId: string,
    data: OpenEditorRequest
  ): Promise<OpenEditorResponse> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/integration/editor/open`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<OpenEditorResponse>(response);
  },

  getEditorPath: async (
    workspaceId: string
  ): Promise<{ workspace_path: string }> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/integration/editor/path`
    );
    return handleApiResponse<{ workspace_path: string }>(response);
  },

  getBranchStatus: async (workspaceId: string): Promise<RepoBranchStatus[]> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/git/status`
    );
    return handleApiResponse<RepoBranchStatus[]>(response);
  },

  getRepos: async (workspaceId: string): Promise<RepoWithTargetBranch[]> => {
    const response = await makeRequest(`/api/workspaces/${workspaceId}/repos`);
    return handleApiResponse<RepoWithTargetBranch[]>(response);
  },

  getFirstUserMessage: async (workspaceId: string): Promise<string | null> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/messages/first`
    );
    return handleApiResponse<string | null>(response);
  },

  merge: async (
    workspaceId: string,
    data: MergeWorkspaceRequest
  ): Promise<void> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/git/merge`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<void>(response);
  },

  push: async (
    workspaceId: string,
    data: PushWorkspaceRequest
  ): Promise<Result<void, PushError>> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/git/push`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponseAsResult<void, PushError>(response);
  },

  forcePush: async (
    workspaceId: string,
    data: PushWorkspaceRequest
  ): Promise<Result<void, PushError>> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/git/push/force`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponseAsResult<void, PushError>(response);
  },

  rebase: async (
    workspaceId: string,
    data: RebaseWorkspaceRequest
  ): Promise<Result<void, GitOperationError>> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/git/rebase`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponseAsResult<void, GitOperationError>(response);
  },

  change_target_branch: async (
    workspaceId: string,
    data: ChangeTargetBranchRequest
  ): Promise<ChangeTargetBranchResponse> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/git/target-branch`,
      {
        method: 'PUT',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<ChangeTargetBranchResponse>(response);
  },

  renameBranch: async (
    workspaceId: string,
    newBranchName: string
  ): Promise<RenameBranchResponse> => {
    const payload: RenameBranchRequest = {
      new_branch_name: newBranchName,
    };
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/git/branch`,
      {
        method: 'PUT',
        body: JSON.stringify(payload),
      }
    );
    return handleApiResponse<RenameBranchResponse>(response);
  },

  abortConflicts: async (
    workspaceId: string,
    data: AbortConflictsRequest
  ): Promise<void> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/git/conflicts/abort`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<void>(response);
  },

  continueRebase: async (
    workspaceId: string,
    data: ContinueRebaseRequest
  ): Promise<void> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/git/rebase/continue`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<void>(response);
  },

  createPR: async (
    workspaceId: string,
    data: CreatePrApiRequest
  ): Promise<Result<string, PrError>> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/pull-requests`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponseAsResult<string, PrError>(response);
  },

  /** Try to auto-attach a PR by matching the workspace branch */
  attachPr: async (
    workspaceId: string,
    data: AttachExistingPrRequest
  ): Promise<Result<AttachPrResponse, PrError>> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/pull-requests/attach`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponseAsResult<AttachPrResponse, PrError>(response);
  },

  startDevServer: async (workspaceId: string): Promise<ExecutionProcess[]> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/execution/dev-server/start`,
      {
        method: 'POST',
      }
    );
    return handleApiResponse<ExecutionProcess[]>(response);
  },

  setupGhCli: async (workspaceId: string): Promise<ExecutionProcess> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/integration/github/cli/setup`,
      {
        method: 'POST',
      }
    );
    return handleApiResponse<ExecutionProcess, GhCliSetupError>(response);
  },

  runSetupScript: async (
    workspaceId: string
  ): Promise<Result<ExecutionProcess, RunScriptError>> => {
    const sessions = await sessionsApi.getByWorkspace(workspaceId);
    const session =
      sessions[0] ??
      (await sessionsApi.create({
        workspace_id: workspaceId,
      }));

    return sessionsApi.runSetupScript(session.id);
  },

  runCleanupScript: async (
    workspaceId: string
  ): Promise<Result<ExecutionProcess, RunScriptError>> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/execution/cleanup`,
      {
        method: 'POST',
      }
    );
    return handleApiResponseAsResult<ExecutionProcess, RunScriptError>(
      response
    );
  },

  runArchiveScript: async (
    workspaceId: string
  ): Promise<Result<ExecutionProcess, RunScriptError>> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/execution/archive`,
      {
        method: 'POST',
      }
    );
    return handleApiResponseAsResult<ExecutionProcess, RunScriptError>(
      response
    );
  },

  getPrComments: async (
    workspaceId: string,
    repoId: string
  ): Promise<PrCommentsResponse> => {
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/pull-requests/comments?repo_id=${encodeURIComponent(repoId)}`
    );
    return handleApiResponse<PrCommentsResponse>(response);
  },

  /** Mark all coding agent turns for a workspace as seen */
  markSeen: async (workspaceId: string): Promise<void> => {
    const response = await makeRequest(`/api/workspaces/${workspaceId}/seen`, {
      method: 'PUT',
    });
    return handleApiResponse<void>(response);
  },

  /** Create a workspace directly from a pull request */
  createFromPr: async (
    data: CreateWorkspaceFromPrBody
  ): Promise<Result<CreateWorkspaceFromPrResponse, CreateFromPrError>> => {
    const response = await makeRequest('/api/workspaces/from-pr', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponseAsResult<
      CreateWorkspaceFromPrResponse,
      CreateFromPrError
    >(response);
  },
};

// Execution Process APIs
export const executionProcessesApi = {
  getDetails: async (processId: string): Promise<ExecutionProcess> => {
    const response = await makeRequest(`/api/execution-processes/${processId}`);
    return handleApiResponse<ExecutionProcess>(response);
  },

  getRepoStates: async (
    processId: string
  ): Promise<ExecutionProcessRepoState[]> => {
    const response = await makeRequest(
      `/api/execution-processes/${processId}/repo-states`
    );
    return handleApiResponse<ExecutionProcessRepoState[]>(response);
  },

  stopExecutionProcess: async (processId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/execution-processes/${processId}/stop`,
      {
        method: 'POST',
      }
    );
    return handleApiResponse<void>(response);
  },
};

// File System APIs
export const fileSystemApi = {
  list: async (path?: string): Promise<DirectoryListResponse> => {
    const queryParam = path ? `?path=${encodeURIComponent(path)}` : '';
    const response = await makeRequest(
      `/api/filesystem/directory${queryParam}`
    );
    return handleApiResponse<DirectoryListResponse>(response);
  },

  listGitRepos: async (path?: string): Promise<DirectoryEntry[]> => {
    const queryParam = path ? `?path=${encodeURIComponent(path)}` : '';
    const response = await makeRequest(
      `/api/filesystem/git-repos${queryParam}`
    );
    return handleApiResponse<DirectoryEntry[]>(response);
  },
};

// Repo APIs
export const repoApi = {
  list: async (): Promise<Repo[]> => {
    const response = await makeRequest('/api/repos');
    return handleApiResponse<Repo[]>(response);
  },

  listRecent: async (): Promise<Repo[]> => {
    const response = await makeRequest('/api/repos/recent');
    return handleApiResponse<Repo[]>(response);
  },

  getById: async (repoId: string): Promise<Repo> => {
    const response = await makeRequest(`/api/repos/${repoId}`);
    return handleApiResponse<Repo>(response);
  },

  update: async (repoId: string, data: UpdateRepo): Promise<Repo> => {
    const response = await makeRequest(`/api/repos/${repoId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Repo>(response);
  },

  delete: async (repoId: string): Promise<void> => {
    const response = await makeRequest(`/api/repos/${repoId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  register: async (data: {
    path: string;
    display_name?: string;
  }): Promise<Repo> => {
    const response = await makeRequest('/api/repos', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Repo>(response);
  },

  getBranches: async (repoId: string): Promise<GitBranch[]> => {
    const response = await makeRequest(`/api/repos/${repoId}/branches`);
    return handleApiResponse<GitBranch[]>(response);
  },

  init: async (data: {
    parent_path: string;
    folder_name: string;
  }): Promise<Repo> => {
    const response = await makeRequest('/api/repos/init', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Repo>(response);
  },

  getBatch: async (ids: string[]): Promise<Repo[]> => {
    const response = await makeRequest('/api/repos/batch', {
      method: 'POST',
      body: JSON.stringify({ ids }),
    });
    return handleApiResponse<Repo[]>(response);
  },

  openEditor: async (
    repoId: string,
    data: OpenEditorRequest
  ): Promise<OpenEditorResponse> => {
    const response = await makeRequest(`/api/repos/${repoId}/open-editor`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<OpenEditorResponse>(response);
  },

  searchFiles: async (
    repoId: string,
    query: string,
    mode?: SearchMode,
    options?: RequestInit
  ): Promise<SearchResult[]> => {
    const modeParam = mode ? `&mode=${encodeURIComponent(mode)}` : '';
    const response = await makeRequest(
      `/api/repos/${repoId}/search?q=${encodeURIComponent(query)}${modeParam}`,
      options
    );
    return handleApiResponse<SearchResult[]>(response);
  },

  listOpenPrs: async (
    repoId: string,
    remoteName?: string
  ): Promise<Result<PullRequestDetail[], ListPrsError>> => {
    const params = remoteName
      ? `?remote=${encodeURIComponent(remoteName)}`
      : '';
    const response = await makeRequest(`/api/repos/${repoId}/prs${params}`);
    return handleApiResponseAsResult<PullRequestDetail[], ListPrsError>(
      response
    );
  },

  listRemotes: async (repoId: string): Promise<GitRemote[]> => {
    const response = await makeRequest(`/api/repos/${repoId}/remotes`);
    return handleApiResponse<GitRemote[]>(response);
  },
};

// Issue PR linking APIs
export const issuePrsApi = {
  getPrInfo: async (
    url: string
  ): Promise<Result<PullRequestDetail, ListPrsError>> => {
    const response = await makeRequest(
      `/api/repos/pr-info?url=${encodeURIComponent(url)}`
    );
    return handleApiResponseAsResult<PullRequestDetail, ListPrsError>(response);
  },

  linkToIssue: async (data: LinkPrToIssueRequest): Promise<void> => {
    const response = await makeRequest('/api/remote/pull-requests/link', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    await handleApiResponse<void>(response);
  },
};

// Config APIs (backwards compatible)
export const configApi = {
  getConfig: async (): Promise<UserSystemInfo> => {
    const response = await makeRequest('/api/info', {
      cache: 'no-store',
    });
    return handleApiResponse<UserSystemInfo>(response);
  },
  saveConfig: async (config: Config): Promise<Config> => {
    const response = await makeRequest('/api/config', {
      method: 'PUT',
      body: JSON.stringify(config),
    });
    return handleApiResponse<Config>(response);
  },
  checkEditorAvailability: async (
    editorType: EditorType
  ): Promise<CheckEditorAvailabilityResponse> => {
    const response = await makeRequest(
      `/api/editors/check-availability?editor_type=${encodeURIComponent(editorType)}`
    );
    return handleApiResponse<CheckEditorAvailabilityResponse>(response);
  },
  checkAgentAvailability: async (
    agent: BaseCodingAgent
  ): Promise<AvailabilityInfo> => {
    const response = await makeRequest(
      `/api/agents/check-availability?executor=${encodeURIComponent(agent)}`
    );
    return handleApiResponse<AvailabilityInfo>(response);
  },
  recheckClaudeSeat: async (): Promise<ClaudeSeatStatus> => {
    const response = await makeRequest('/api/claude-seat/recheck', {
      method: 'POST',
    });
    return handleApiResponse<ClaudeSeatStatus>(response);
  },
  getClaudeConnectors: async (): Promise<ClaudeConnectorsResponse> => {
    const response = await makeRequest('/api/claude-connectors');
    return handleApiResponse<ClaudeConnectorsResponse>(response);
  },
  getUsage: async (days?: number): Promise<UsageSummary> => {
    const suffix = days != null ? `?days=${days}` : '';
    const response = await makeRequest(`/api/usage${suffix}`);
    return handleApiResponse<UsageSummary>(response);
  },
};

// Task Tags APIs (all tags are global)
export const tagsApi = {
  list: async (params?: TagSearchParams): Promise<Tag[]> => {
    const queryParam = params?.search
      ? `?search=${encodeURIComponent(params.search)}`
      : '';
    const response = await makeRequest(`/api/tags${queryParam}`);
    return handleApiResponse<Tag[]>(response);
  },

  create: async (data: CreateTag): Promise<Tag> => {
    const response = await makeRequest('/api/tags', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Tag>(response);
  },

  update: async (tagId: string, data: UpdateTag): Promise<Tag> => {
    const response = await makeRequest(`/api/tags/${tagId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Tag>(response);
  },

  delete: async (tagId: string): Promise<void> => {
    const response = await makeRequest(`/api/tags/${tagId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },
};

// MCP Servers APIs
export const mcpServersApi = {
  load: async (query: McpServerQuery): Promise<GetMcpServerResponse> => {
    const params = new URLSearchParams(query);
    const response = await makeRequest(`/api/mcp-config?${params.toString()}`);
    return handleApiResponse<GetMcpServerResponse>(response);
  },
  save: async (
    query: McpServerQuery,
    data: UpdateMcpServersBody
  ): Promise<void> => {
    const params = new URLSearchParams(query);
    // params.set('profile', profile);
    const response = await makeRequest(`/api/mcp-config?${params.toString()}`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    if (!response.ok) {
      const errorData = await response.json();
      console.error('[API Error] Failed to save MCP servers', {
        message: errorData.message,
        status: response.status,
        response,
        timestamp: new Date().toISOString(),
      });
      throw new ApiError(
        errorData.message || 'Failed to save MCP servers',
        response.status,
        response
      );
    }
  },
};

// Profiles API
export const profilesApi = {
  load: async (): Promise<{ content: string; path: string }> => {
    const response = await makeRequest('/api/profiles');
    return handleApiResponse<{ content: string; path: string }>(response);
  },
  save: async (content: string): Promise<string> => {
    const response = await makeRequest('/api/profiles', {
      method: 'PUT',
      body: content,
      headers: {
        'Content-Type': 'application/json',
      },
    });
    return handleApiResponse<string>(response);
  },
};

// Workspace attachments API
export const attachmentsApi = {
  upload: async (attachment: File): Promise<AttachmentResponse> => {
    const formData = new FormData();
    formData.append('image', attachment);

    const response = await makeLocalApiRequest('/api/attachments/upload', {
      method: 'POST',
      body: formData,
      credentials: 'include',
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new ApiError(
        `Failed to upload attachment: ${errorText}`,
        response.status,
        response
      );
    }

    return handleApiResponse<AttachmentResponse>(response);
  },

  uploadForTask: async (
    taskId: string,
    attachment: File
  ): Promise<AttachmentResponse> => {
    const formData = new FormData();
    formData.append('image', attachment);

    const response = await makeLocalApiRequest(
      `/api/attachments/task/${taskId}/upload`,
      {
        method: 'POST',
        body: formData,
        credentials: 'include',
      }
    );

    if (!response.ok) {
      const errorText = await response.text();
      throw new ApiError(
        `Failed to upload attachment: ${errorText}`,
        response.status,
        response
      );
    }

    return handleApiResponse<AttachmentResponse>(response);
  },

  uploadForAttempt: async (
    workspaceId: string,
    sessionId: string,
    attachment: File
  ): Promise<AttachmentResponse> => {
    const formData = new FormData();
    formData.append('image', attachment);

    const response = await makeLocalApiRequest(
      `/api/workspaces/${workspaceId}/attachments/upload?session_id=${sessionId}`,
      {
        method: 'POST',
        body: formData,
        credentials: 'include',
      }
    );

    if (!response.ok) {
      const errorText = await response.text();
      throw new ApiError(
        `Failed to upload attachment: ${errorText}`,
        response.status,
        response
      );
    }

    return handleApiResponse<AttachmentResponse>(response);
  },

  delete: async (attachmentId: string): Promise<void> => {
    const response = await makeRequest(`/api/attachments/${attachmentId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  getTaskAttachments: async (taskId: string): Promise<AttachmentResponse[]> => {
    const response = await makeRequest(`/api/attachments/task/${taskId}`);
    return handleApiResponse<AttachmentResponse[]>(response);
  },

  getAttachmentUrl: (attachmentId: string): string => {
    return `/api/attachments/${attachmentId}/file`;
  },
};

// Approval API
export const approvalsApi = {
  respond: async (
    approvalId: string,
    payload: ApprovalResponse,
    signal?: AbortSignal
  ): Promise<ApprovalStatus> => {
    const res = await makeRequest(`/api/approvals/${approvalId}/respond`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
      signal,
    });

    return handleApiResponse<ApprovalStatus>(res);
  },
};

// Board auth: the server signs board tokens with its board secret; there is
// no sign-in.
export const authApi = {
  /** Returns a freshly signed token for the board (an owner token). */
  getToken: async (): Promise<TokenResponse> => {
    const response = await makeRequest('/api/auth/token');
    if (response.status === 401) {
      throw new ApiError('Unauthorized', 401, response);
    }
    return handleApiResponse<TokenResponse>(response);
  },

  /** Returns the user ID of the board's owner, whom the server acts as. */
  getCurrentUser: async (): Promise<CurrentUserResponse> => {
    const response = await makeRequest('/api/auth/user');
    return handleApiResponse<CurrentUserResponse>(response);
  },
};

/**
 * @deprecated Use `tokenManager.getToken()` from
 * `@/shared/lib/auth/tokenManager` instead.
 * This function does not handle 401 responses or token refresh coordination.
 */
export async function getCachedToken(): Promise<string | null> {
  const { tokenManager } = await import('@/shared/lib/auth/tokenManager');
  return tokenManager.getToken();
}

const handleRemoteResponse = async <T>(response: Response): Promise<T> => {
  if (!response.ok) {
    let errorMessage = `Request failed with status ${response.status}`;

    try {
      const body = (await response.json()) as {
        error?: string;
        message?: string;
      };
      errorMessage = body.error || body.message || errorMessage;
    } catch {
      errorMessage = response.statusText || errorMessage;
    }

    throw new ApiError(errorMessage, response.status, response);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return response.json() as Promise<T>;
};

// Agent users (admin-managed automation identities)
export interface AgentUserT {
  id: string;
  email: string;
  first_name: string | null;
  last_name: string | null;
  username: string | null;
  created_at: string;
}

export interface AgentUserWithSecretT {
  user: AgentUserT;
  /** Returned exactly once — store it in the runner's config. */
  secret: string;
}

export const agentUsersApi = {
  list: async (orgId: string): Promise<AgentUserT[]> => {
    const response = await makeRemoteRequest(
      `/v1/organizations/${orgId}/agent_users`
    );
    const result = await handleRemoteResponse<{ agents: AgentUserT[] }>(
      response
    );
    return result.agents;
  },

  create: async (
    orgId: string,
    data: { email: string; name: string }
  ): Promise<AgentUserWithSecretT> => {
    const response = await makeRemoteRequest(
      `/v1/organizations/${orgId}/agent_users`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(data),
      }
    );
    return handleRemoteResponse<AgentUserWithSecretT>(response);
  },

  rotate: async (
    orgId: string,
    userId: string
  ): Promise<AgentUserWithSecretT> => {
    const response = await makeRemoteRequest(
      `/v1/organizations/${orgId}/agent_users/${userId}/rotate`,
      { method: 'POST' }
    );
    return handleRemoteResponse<AgentUserWithSecretT>(response);
  },
};

// Organizations API
export const organizationsApi = {
  getMembers: async (
    orgId: string
  ): Promise<OrganizationMemberWithProfile[]> => {
    const response = await makeRemoteRequest(
      `/v1/organizations/${orgId}/members`
    );
    const result = await handleRemoteResponse<ListMembersResponse>(response);
    return result.members;
  },

  getUserOrganizations: async (): Promise<ListOrganizationsResponse> => {
    const response = await makeRemoteRequest('/v1/organizations');
    return handleRemoteResponse<ListOrganizationsResponse>(response);
  },
};

export const remoteProjectsApi = {
  listByOrganization: async (
    organizationId: string
  ): Promise<RemoteProject[]> => {
    const response = await makeRequest(
      `/api/remote/projects?organization_id=${encodeURIComponent(organizationId)}`
    );
    const result =
      await handleApiResponse<ListRemoteProjectsResponse>(response);
    return result.projects;
  },
};

// Scratch API
export const scratchApi = {
  create: async (
    scratchType: ScratchType,
    id: string,
    data: CreateScratch
  ): Promise<Scratch> => {
    const response = await makeRequest(`/api/scratch/${scratchType}/${id}`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Scratch>(response);
  },

  get: async (scratchType: ScratchType, id: string): Promise<Scratch> => {
    const response = await makeRequest(`/api/scratch/${scratchType}/${id}`);
    return handleApiResponse<Scratch>(response);
  },

  update: async (
    scratchType: ScratchType,
    id: string,
    data: UpdateScratch
  ): Promise<void> => {
    const response = await makeRequest(`/api/scratch/${scratchType}/${id}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
    return handleApiResponse<void>(response);
  },

  delete: async (scratchType: ScratchType, id: string): Promise<void> => {
    const response = await makeRequest(`/api/scratch/${scratchType}/${id}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  getStreamUrl: (scratchType: ScratchType, id: string): string =>
    `/api/scratch/${scratchType}/${id}/stream/ws`,
};

// Agents API
export const agentsApi = {
  getDiscoveredOptionsStreamUrl: (
    agent: BaseCodingAgent,
    opts?: { workspaceId?: string; sessionId?: string; repoId?: string }
  ): string => {
    const params = new URLSearchParams();
    params.set('executor', agent);
    if (opts?.workspaceId) params.set('workspace_id', opts.workspaceId);
    if (opts?.sessionId) params.set('session_id', opts.sessionId);
    if (opts?.repoId) params.set('repo_id', opts.repoId);

    return `/api/agents/discovered-options/ws?${params.toString()}`;
  },

  getPresetOptions: async (
    query: AgentPresetOptionsQuery
  ): Promise<ExecutorConfig> => {
    const params = new URLSearchParams();
    params.set('executor', query.executor);
    if (query.variant) params.set('variant', query.variant);
    const response = await makeRequest(
      `/api/agents/preset-options?${params.toString()}`
    );
    return handleApiResponse<ExecutorConfig>(response);
  },
};

// Queue API for session follow-up messages
export const queueApi = {
  /**
   * Queue a follow-up message to be executed when current execution finishes
   */
  queue: async (
    sessionId: string,
    data: DraftFollowUpData
  ): Promise<QueueStatus> => {
    const response = await makeRequest(`/api/sessions/${sessionId}/queue`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<QueueStatus>(response);
  },

  /**
   * Cancel a queued follow-up message
   */
  cancel: async (sessionId: string): Promise<QueueStatus> => {
    const response = await makeRequest(`/api/sessions/${sessionId}/queue`, {
      method: 'DELETE',
    });
    return handleApiResponse<QueueStatus>(response);
  },

  /**
   * Get the current queue status for a session
   */
  getStatus: async (sessionId: string): Promise<QueueStatus> => {
    const response = await makeRequest(`/api/sessions/${sessionId}/queue`);
    return handleApiResponse<QueueStatus>(response);
  },
};

// Releases API (GitHub releases proxy)
export interface GitHubRelease {
  name: string;
  tag_name: string;
  published_at: string;
  body: string;
}

interface ReleasesResponse {
  releases: GitHubRelease[];
}

export const releasesApi = {
  list: async (): Promise<GitHubRelease[]> => {
    const response = await makeRequest('/api/releases');
    const result = await handleApiResponse<ReleasesResponse>(response);
    return result.releases;
  },
};

// Tracker import API
export const trackerApi = {
  /** Fetch importable tracker tickets for a project */
  preview: async (
    data: TrackerPreviewRequest
  ): Promise<Result<TrackerPreviewResponse, TrackerImportError>> => {
    const response = await makeRequest('/api/tracker/preview', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponseAsResult<
      TrackerPreviewResponse,
      TrackerImportError
    >(response);
  },

  /** Everyone assignable in the configured project, for the approver picker */
  assignableUsers: async (): Promise<Record<string, string>> => {
    const response = await makeRequest('/api/tracker/assignable-users');
    return handleApiResponse<Record<string, string>>(response);
  },

  /** Import the selected tickets into a project */
  import: async (
    data: TrackerImportRequest
  ): Promise<Result<TrackerImportResponse, TrackerImportError>> => {
    const response = await makeRequest('/api/tracker/import', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponseAsResult<TrackerImportResponse, TrackerImportError>(
      response
    );
  },
};

// Search API (multi-repo file search)
export const searchApi = {
  searchFiles: async (
    repoIds: string[],
    query: string,
    mode?: SearchMode,
    options?: RequestInit
  ): Promise<SearchResult[]> => {
    const repoIdsParam = repoIds.join(',');
    const modeParam = mode ? `&mode=${encodeURIComponent(mode)}` : '';
    const response = await makeRequest(
      `/api/search?q=${encodeURIComponent(query)}&repo_ids=${encodeURIComponent(repoIdsParam)}${modeParam}`,
      options
    );
    return handleApiResponse<SearchResult[]>(response);
  },
};

// Script runner: a repository's `.prepcook/scripts.toml` and test runs of it
export const scriptsApi = {
  getRepoScripts: async (repoId: string): Promise<RepoScriptsResponse> => {
    const response = await makeRequest(`/api/repos/${repoId}/scripts`);
    return handleApiResponse<RepoScriptsResponse>(response);
  },

  run: async (repoId: string, step: RunStep): Promise<RunScriptResponse> => {
    const body: RunScriptRequest = { step };
    const response = await makeRequest(`/api/repos/${repoId}/scripts/run`, {
      method: 'POST',
      body: JSON.stringify(body),
    });
    return handleApiResponse<RunScriptResponse>(response);
  },

  getRun: async (runId: string): Promise<ScriptRunState> => {
    const response = await makeRequest(`/api/script-runs/${runId}`);
    return handleApiResponse<ScriptRunState>(response);
  },
};

// Playbook context: the repository and product values a board's playbooks
// are filled with
export const playbooksApi = {
  /** Placeholder name to value, for `resolveStagePlaybook`'s `context`. */
  getContext: async (
    projectId: string | null | undefined
  ): Promise<Record<string, string>> => {
    const query = projectId
      ? `?project_id=${encodeURIComponent(projectId)}`
      : '';
    const response = await makeRequest(`/api/playbooks/context${query}`);
    const { vars } = await handleApiResponse<PlaybookContextResponse>(response);
    const context: Record<string, string> = {};
    for (const [name, value] of Object.entries(vars)) {
      if (value !== undefined) context[name] = value;
    }
    return context;
  },
};

// Secrets store: names and the file path go out, values only come in
// Integration areas: the providers' descriptors Settings renders from, and
// their connection tests
export const integrationsApi = {
  /** Every area's provider descriptors, keyed by area. */
  descriptors: async (): Promise<Record<string, Descriptor[]>> => {
    const response = await makeRequest('/api/integrations/descriptors');
    return handleApiResponse<Record<string, Descriptor[]>>(response);
  },

  /** Runs one provider's test against `fields` (unsaved values welcome). */
  test: async (
    area: string,
    kind: string,
    fields?: IntegrationTestRequest['fields']
  ): Promise<TestOutcome> => {
    const body: IntegrationTestRequest = { fields };
    const response = await makeRequest(
      `/api/integrations/${encodeURIComponent(area)}/${encodeURIComponent(
        kind
      )}/test`,
      { method: 'POST', body: JSON.stringify(body) }
    );
    return handleApiResponse<TestOutcome>(response);
  },
};

export const secretsApi = {
  list: async (): Promise<SecretsResponse> => {
    const response = await makeRequest('/api/secrets');
    return handleApiResponse<SecretsResponse>(response);
  },

  set: async (name: string, value: string): Promise<SecretsResponse> => {
    const body: SetSecretRequest = { value };
    const response = await makeRequest(
      `/api/secrets/${encodeURIComponent(name)}`,
      { method: 'PUT', body: JSON.stringify(body) }
    );
    return handleApiResponse<SecretsResponse>(response);
  },

  remove: async (name: string): Promise<SecretsResponse> => {
    const response = await makeRequest(
      `/api/secrets/${encodeURIComponent(name)}`,
      { method: 'DELETE' }
    );
    return handleApiResponse<SecretsResponse>(response);
  },
};
