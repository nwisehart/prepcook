import { useParams } from '@tanstack/react-router';
import { KnowledgeView } from './KnowledgeView';

/** Full-page Knowledge (RCA graph) view, routed at /projects/:id/knowledge. */
export function KnowledgePage() {
  const { projectId } = useParams({ strict: false }) as {
    projectId?: string;
  };

  if (!projectId) {
    return null;
  }

  return (
    <div className="h-full overflow-auto bg-primary">
      <div className="flex min-h-full flex-col px-7 pb-7 pt-[22px]">
        <KnowledgeView projectId={projectId} />
      </div>
    </div>
  );
}
