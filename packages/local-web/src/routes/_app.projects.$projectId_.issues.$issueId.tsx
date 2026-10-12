import { createFileRoute } from '@tanstack/react-router';
import { LocalProjectKanban } from '@/pages/kanban/LocalProjectKanban';
import { projectSearchValidator } from '@prepcook/web-core/project-search';

export const Route = createFileRoute(
  '/_app/projects/$projectId_/issues/$issueId'
)({
  validateSearch: projectSearchValidator,
  component: LocalProjectKanban,
});
