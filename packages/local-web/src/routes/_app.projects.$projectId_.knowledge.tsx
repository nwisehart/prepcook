import { createFileRoute } from '@tanstack/react-router';
import { KnowledgePage } from '@/pages/knowledge/KnowledgePage';

export const Route = createFileRoute('/_app/projects/$projectId_/knowledge')({
  component: KnowledgePage,
});
