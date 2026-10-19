import type { DropResult } from '@hello-pangea/dnd';
import {
  GraphIcon,
  KanbanIcon,
  SlidersHorizontalIcon,
  TerminalWindowIcon,
  XIcon,
} from '@phosphor-icons/react';
import { useTranslation } from 'react-i18next';
import { cn } from '../lib/cn';
import { AppBarButton } from './AppBarButton';
import { ModeCard, type OrchestratorModeName } from './ModeCard';
import { PrepCookMark } from './PrepCookMark';
import { ProjectSwitcher, type AppBarProject } from './ProjectSwitcher';

export type { AppBarProject } from './ProjectSwitcher';
export type { OrchestratorModeName } from './ModeCard';

/** `up`: the board answers. `down`: unreachable or not configured. */
export type AppBarBoardStatus = 'up' | 'down';

interface AppBarProps {
  // Project switcher
  projects: AppBarProject[];
  /** Project named by the switcher and opened by Board. */
  currentProjectId: string | null;
  onProjectClick: (projectId: string) => void;
  onProjectsDragEnd: (result: DropResult) => void;
  isSavingProjectOrder?: boolean;
  onCreateProject?: () => void;
  hasBoardAccess?: boolean;
  isLoadingProjects?: boolean;
  onOpenBoardSettings?: () => void;
  onExportClick?: () => void;
  isExportActive?: boolean;

  // Nav items. An item without a handler renders disabled (Board, Knowledge)
  // or not at all (Settings).
  onBoardClick?: () => void;
  isBoardActive?: boolean;
  onWorkspacesClick: () => void;
  isWorkspacesActive: boolean;
  showWorkspacesButton?: boolean;
  /** Running sessions; the Sessions item shows a count chip when above 0. */
  runningSessionsCount?: number;
  onKnowledgeClick?: () => void;
  isKnowledgeActive?: boolean;
  onSettingsClick?: () => void;

  // Mode card, hidden while the mode is unknown
  mode?: OrchestratorModeName | null;
  onSwitchMode?: () => void;

  // Footer
  appVersion?: string | null;
  updateVersion?: string | null;
  onUpdateClick?: () => void;
  boardStatus?: AppBarBoardStatus | null;

  onHoverStart?: () => void;
  onHoverEnd?: () => void;
  /** Shows a close button in the brand row (mobile drawer). */
  onClose?: () => void;
  /**
   * Reserves a draggable strip at the top for the macOS window controls,
   * which the desktop app overlays on the top-left corner.
   */
  windowControlsInset?: boolean;
  className?: string;
}

/**
 * The 232px app sidebar: brand row, project switcher, primary nav, the
 * orchestrator mode card and a footer with the version and board status.
 */
export function AppBar({
  projects,
  currentProjectId,
  onProjectClick,
  onProjectsDragEnd,
  isSavingProjectOrder,
  onCreateProject,
  hasBoardAccess,
  isLoadingProjects,
  onOpenBoardSettings,
  onExportClick,
  isExportActive,
  onBoardClick,
  isBoardActive = false,
  onWorkspacesClick,
  isWorkspacesActive,
  showWorkspacesButton = true,
  runningSessionsCount = 0,
  onKnowledgeClick,
  isKnowledgeActive = false,
  onSettingsClick,
  mode,
  onSwitchMode,
  appVersion,
  updateVersion,
  onUpdateClick,
  boardStatus,
  onHoverStart,
  onHoverEnd,
  onClose,
  windowControlsInset = false,
  className,
}: AppBarProps) {
  const { t } = useTranslation('common');
  const runningLabel = t('appBar.nav.runningSessions', {
    count: runningSessionsCount,
  });

  return (
    <aside
      onMouseEnter={onHoverStart}
      onMouseLeave={onHoverEnd}
      className={cn(
        'flex h-full min-h-0 w-full flex-col gap-1.5 overflow-y-auto',
        'border-r border-border bg-primary px-3.5 pb-4',
        windowControlsInset ? 'pt-0' : 'pt-[18px]',
        className
      )}
    >
      {windowControlsInset && (
        <div data-tauri-drag-region className="h-10 shrink-0" />
      )}

      <div className="flex items-center gap-2.5 px-2 pb-3.5 pt-1">
        <PrepCookMark className="h-[26px] w-[26px] text-brand dark:text-brand-secondary" />
        <span className="text-[17px] font-bold tracking-[-0.2px] text-high">
          {t('appBar.productName')}
        </span>
        {onClose && (
          <button
            type="button"
            onClick={onClose}
            aria-label={t('appBar.closeMenu')}
            className="ml-auto flex size-8 items-center justify-center rounded text-low transition-colors hover:bg-panel hover:text-high cursor-pointer focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand"
          >
            <XIcon className="size-4" />
          </button>
        )}
      </div>

      <ProjectSwitcher
        projects={projects}
        currentProjectId={currentProjectId}
        onProjectClick={onProjectClick}
        onProjectsDragEnd={onProjectsDragEnd}
        isSavingProjectOrder={isSavingProjectOrder}
        onCreateProject={hasBoardAccess ? onCreateProject : undefined}
        hasBoardAccess={hasBoardAccess}
        isLoadingProjects={isLoadingProjects}
        onOpenBoardSettings={onOpenBoardSettings}
        onExportClick={hasBoardAccess ? onExportClick : undefined}
        isExportActive={isExportActive}
      />

      <nav
        aria-label={t('appBar.nav.label')}
        className="mt-2.5 flex flex-col gap-0.5"
      >
        <AppBarButton
          icon={KanbanIcon}
          label={t('appBar.nav.board')}
          isActive={isBoardActive}
          onClick={onBoardClick}
          disabled={!onBoardClick}
        />
        {showWorkspacesButton && (
          <AppBarButton
            icon={TerminalWindowIcon}
            label={t('appBar.nav.sessions')}
            isActive={isWorkspacesActive}
            onClick={onWorkspacesClick}
            badge={
              runningSessionsCount > 0 ? (
                <span
                  title={runningLabel}
                  aria-label={runningLabel}
                  className="inline-flex h-[22px] min-w-[22px] shrink-0 items-center justify-center rounded-full bg-brand-soft px-2 text-xs font-medium text-brand-soft-foreground"
                >
                  {runningSessionsCount}
                </span>
              ) : undefined
            }
          />
        )}
        <AppBarButton
          icon={GraphIcon}
          label={t('kanban.knowledgeMenu')}
          isActive={isKnowledgeActive}
          onClick={onKnowledgeClick}
          disabled={!onKnowledgeClick}
        />
        {onSettingsClick && (
          <AppBarButton
            icon={SlidersHorizontalIcon}
            label={t('settings:settings.layout.nav.title')}
            onClick={onSettingsClick}
          />
        )}
      </nav>

      <div className="min-h-4 flex-1" />

      {mode && <ModeCard mode={mode} onSwitchMode={onSwitchMode} />}

      <div className="flex min-h-[26px] items-center justify-between gap-2 px-2 pt-2.5 text-xs text-low">
        {updateVersion ? (
          <button
            type="button"
            onClick={onUpdateClick}
            disabled={!onUpdateClick}
            className="truncate rounded-sm font-semibold text-brand-secondary hover:underline underline-offset-2 cursor-pointer disabled:cursor-default disabled:no-underline focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand"
          >
            {t('appBar.updateTo', { version: updateVersion })}
          </button>
        ) : appVersion ? (
          <span className="truncate font-mono" title={`v${appVersion}`}>
            v{appVersion}
          </span>
        ) : (
          <span />
        )}
        {boardStatus && (
          <span className="inline-flex shrink-0 items-center gap-1.5">
            <span
              aria-hidden="true"
              className={cn(
                'size-2 rounded-full',
                boardStatus === 'up' ? 'bg-success' : 'bg-error'
              )}
            />
            {boardStatus === 'up'
              ? t('appBar.board.up')
              : t('appBar.board.down')}
          </span>
        )}
      </div>
    </aside>
  );
}
