export const workspaceSessionKeys = {
  byWorkspace: (workspaceId: string | undefined) =>
    ['workspaceSessions', 'local', workspaceId] as const,
};
