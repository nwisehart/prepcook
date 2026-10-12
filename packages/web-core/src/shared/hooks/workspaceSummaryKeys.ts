export const workspaceSummaryKeys = {
  all: ['workspace-summaries'] as const,
  byArchived: (archived: boolean) =>
    ['workspace-summaries', 'local', archived ? 'archived' : 'active'] as const,
};
