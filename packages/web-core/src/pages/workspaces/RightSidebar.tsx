import { memo, useEffect, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { ArrowsOutSimpleIcon } from '@phosphor-icons/react';
import {
  ButtonGroup,
  ButtonGroupItem,
} from '@prepcook/ui/components/IconButtonGroup';
import { ProcessListContainer } from './ProcessListContainer';
import { PreviewControlsContainer } from './PreviewControlsContainer';
import { GitPanelContainer } from './GitPanelContainer';
import { ChangesSummaryContainer } from './ChangesSummaryContainer';
import { TerminalPanelContainer } from '@/shared/components/TerminalPanelContainer';
import { useLogsPanel } from '@/shared/hooks/useLogsPanel';
import { usePreviewDevServer } from '@/features/workspace/model/hooks/usePreviewDevServer';
import { cn } from '@/shared/lib/utils';
import type { RepoWithTargetBranch, Workspace } from 'shared/types';
import {
  RIGHT_MAIN_PANEL_MODES,
  type RightMainPanelMode,
  useUiPreferencesStore,
} from '@/shared/stores/useUiPreferencesStore';

type RightPanelTab = 'review' | 'terminal' | 'preview';

export interface RightSidebarProps {
  rightMainPanelMode: RightMainPanelMode | null;
  selectedWorkspace: Workspace | undefined;
  repos: RepoWithTargetBranch[];
}

/** A quiet bordered card with a small header line. */
function PanelCard({
  title,
  actions,
  className,
  children,
}: {
  title: string;
  actions?: ReactNode;
  className?: string;
  children: ReactNode;
}) {
  return (
    <section
      className={cn(
        'flex min-h-0 flex-col overflow-hidden rounded-lg border bg-secondary',
        className
      )}
    >
      <div className="flex items-center justify-between gap-2 px-3 pb-1 pt-2.5">
        <span className="text-sm font-bold text-high">{title}</span>
        {actions}
      </div>
      <div className="flex min-h-0 flex-1">{children}</div>
    </section>
  );
}

/**
 * The session's right panel: a segmented control over Review (changes and
 * repositories), Terminal (the worktree's shell, on the console surface) and,
 * while a dev server exists, Preview.
 */
export const RightSidebar = memo(function RightSidebar({
  rightMainPanelMode,
  selectedWorkspace,
  repos,
}: RightSidebarProps) {
  const { t } = useTranslation('common');
  const isTerminalVisible = useUiPreferencesStore((s) => s.isTerminalVisible);
  const { expandTerminal, collapseTerminal, isTerminalExpanded } =
    useLogsPanel();
  const { devServerProcesses } = usePreviewDevServer(selectedWorkspace?.id);

  const [tab, setTab] = useState<RightPanelTab>('review');

  // Follow the large panel: opening Logs shows the process list on the
  // Terminal tab, opening Preview shows the dev server controls.
  useEffect(() => {
    if (rightMainPanelMode === RIGHT_MAIN_PANEL_MODES.LOGS) {
      setTab('terminal');
    } else if (rightMainPanelMode === RIGHT_MAIN_PANEL_MODES.PREVIEW) {
      setTab('preview');
    }
  }, [rightMainPanelMode]);

  const hasPreview =
    !!selectedWorkspace &&
    (devServerProcesses.length > 0 ||
      rightMainPanelMode === RIGHT_MAIN_PANEL_MODES.PREVIEW);
  const activeTab: RightPanelTab =
    tab === 'preview' && !hasPreview ? 'review' : tab;

  return (
    <aside
      aria-label={t('workspaces.rightPanel.label')}
      className="flex h-full flex-col gap-3 overflow-hidden border-l bg-surface-raised px-4 pb-[18px] pt-3.5"
    >
      <ButtonGroup className="self-start">
        <ButtonGroupItem
          active={activeTab === 'review'}
          onClick={() => setTab('review')}
          className="h-8"
        >
          {t('workspaces.rightPanel.reviewTab')}
        </ButtonGroupItem>
        <ButtonGroupItem
          active={activeTab === 'terminal'}
          onClick={() => setTab('terminal')}
          className="h-8"
        >
          {t('sections.terminal')}
        </ButtonGroupItem>
        {hasPreview && (
          <ButtonGroupItem
            active={activeTab === 'preview'}
            onClick={() => setTab('preview')}
            className="h-8"
          >
            {t('workspaces.rightPanel.previewTab')}
          </ButtonGroupItem>
        )}
      </ButtonGroup>

      {activeTab === 'review' && (
        <div className="-mx-1 flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-1">
          <ChangesSummaryContainer
            selectedWorkspace={selectedWorkspace}
            repos={repos}
          />
          <GitPanelContainer
            selectedWorkspace={selectedWorkspace}
            repos={repos}
          />
        </div>
      )}

      {activeTab === 'terminal' && (
        <div className="flex min-h-0 flex-1 flex-col gap-3">
          {rightMainPanelMode === RIGHT_MAIN_PANEL_MODES.LOGS && (
            <PanelCard
              title={t('sections.processes')}
              className="max-h-[45%] shrink-0"
            >
              <ProcessListContainer />
            </PanelCard>
          )}
          {isTerminalVisible &&
            (isTerminalExpanded ? (
              <div className="flex flex-col items-start gap-2 rounded-lg border bg-secondary p-3 text-[12.5px] text-low">
                <span>{t('workspaces.rightPanel.terminalExpanded')}</span>
                <button
                  type="button"
                  onClick={collapseTerminal}
                  className={cn(
                    'inline-flex h-8 items-center rounded border bg-secondary px-3 text-sm font-semibold text-high transition-colors hover:bg-panel',
                    'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
                  )}
                >
                  {t('workspaces.rightPanel.showTerminalHere')}
                </button>
              </div>
            ) : (
              <section className="flex min-h-[150px] flex-1 flex-col gap-1.5 overflow-hidden rounded-lg border bg-[hsl(var(--console-background))] p-3 text-[hsl(var(--console-foreground))]">
                <div className="flex items-center justify-between gap-2">
                  <span className="text-[12.5px] font-semibold text-[hsl(var(--console-foreground)/0.7)]">
                    {t('workspaces.rightPanel.terminalTitle')}
                  </span>
                  <button
                    type="button"
                    onClick={expandTerminal}
                    aria-label={t('workspaces.rightPanel.expandTerminal')}
                    title={t('workspaces.rightPanel.expandTerminal')}
                    className={cn(
                      'flex size-6 items-center justify-center rounded-sm text-[hsl(var(--console-foreground)/0.7)] transition-colors hover:text-[hsl(var(--console-foreground))]',
                      'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
                    )}
                  >
                    <ArrowsOutSimpleIcon className="size-3.5" />
                  </button>
                </div>
                <div className="-mx-2 -mb-1 flex min-h-0 flex-1">
                  <TerminalPanelContainer />
                </div>
              </section>
            ))}
        </div>
      )}

      {activeTab === 'preview' && selectedWorkspace && (
        <PanelCard title={t('sections.devServer')} className="flex-1">
          <PreviewControlsContainer
            workspaceId={selectedWorkspace.id}
            className=""
          />
        </PanelCard>
      )}
    </aside>
  );
});
