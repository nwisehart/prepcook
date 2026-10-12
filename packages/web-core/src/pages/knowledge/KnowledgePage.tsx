import { useTranslation } from 'react-i18next';
import { useParams } from '@tanstack/react-router';
import { ArrowLeft } from 'lucide-react';
import { useAppNavigation } from '@/shared/hooks/useAppNavigation';
import { KnowledgeView } from './KnowledgeView';

/** Full-page Knowledge (RCA graph) view, routed at /projects/:id/knowledge. */
export function KnowledgePage() {
  const { t } = useTranslation('common');
  const appNavigation = useAppNavigation();
  const { projectId } = useParams({ strict: false }) as {
    projectId?: string;
  };

  if (!projectId) {
    return null;
  }

  return (
    <div className="h-full overflow-auto">
      <div className="max-w-5xl mx-auto p-4 space-y-4">
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => appNavigation.goToProject(projectId)}
            className="p-1 rounded-sm text-low hover:text-normal hover:bg-secondary transition-colors"
            aria-label={t('kanban.knowledge.backToBoard', 'Back to board')}
          >
            <ArrowLeft className="w-4 h-4" />
          </button>
          <h1 className="text-base font-medium">
            {t('kanban.knowledge.title', 'Knowledge — RCA graph')}
          </h1>
        </div>
        <KnowledgeView projectId={projectId} />
      </div>
    </div>
  );
}
