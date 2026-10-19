import { Fragment, type ReactNode, type RefObject } from 'react';
import { useTranslation } from 'react-i18next';
import { ArrowDownIcon, SpinnerIcon } from '@phosphor-icons/react';
import { cn } from '../lib/cn';

export interface WorkspacesMainWorkspace {
  id: string;
}

export interface WorkspacesMainBreadcrumb {
  label: string;
  /** Render in the mono face (card keys) */
  mono?: boolean;
  onClick?: () => void;
}

interface WorkspacesMainHeaderProps {
  breadcrumbs: WorkspacesMainBreadcrumb[];
  title: string;
  /** Label of the amber "Needs you" chip; the chip is hidden when unset */
  needsYouLabel?: string;
  branch?: string;
  /** Model and effort, e.g. "Opus · high" */
  modelLabel?: string;
  /** Header buttons (secondary actions, More menu) */
  actions?: ReactNode;
}

const headerChipClassName =
  'inline-flex h-6 min-w-0 max-w-[260px] items-center rounded-full border bg-secondary px-2 text-xs font-medium text-normal';

/**
 * The session header: breadcrumb (project / card key / stage), the title on
 * one line, then the session's chips and actions.
 */
export function WorkspacesMainHeader({
  breadcrumbs,
  title,
  needsYouLabel,
  branch,
  modelLabel,
  actions,
}: WorkspacesMainHeaderProps) {
  const { t } = useTranslation('common');

  return (
    <header className="flex flex-wrap items-center gap-2.5 border-b px-6 pb-3.5 pt-[18px]">
      <div className="mr-auto flex min-w-[200px] flex-1 flex-col gap-1">
        {breadcrumbs.length > 0 && (
          <nav
            aria-label={t('workspaces.header.breadcrumb')}
            className="flex min-w-0 items-center gap-2 text-[12.5px] text-low"
          >
            {breadcrumbs.map((crumb, index) => (
              <Fragment key={`${crumb.label}-${index}`}>
                {index > 0 && <span aria-hidden>/</span>}
                {crumb.onClick ? (
                  <button
                    type="button"
                    onClick={crumb.onClick}
                    className={cn(
                      'min-w-0 truncate rounded-sm hover:underline',
                      'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
                      // Card keys read as part of the trail; names read as links.
                      crumb.mono
                        ? 'font-mono text-low hover:text-high'
                        : 'text-brand-secondary'
                    )}
                  >
                    {crumb.label}
                  </button>
                ) : (
                  <span
                    className={cn(
                      'min-w-0 truncate',
                      crumb.mono && 'font-mono'
                    )}
                  >
                    {crumb.label}
                  </span>
                )}
              </Fragment>
            ))}
          </nav>
        )}
        <h1 className="truncate text-lg font-bold tracking-[-0.2px] text-high">
          {title}
        </h1>
      </div>
      {needsYouLabel && (
        <span className="inline-flex h-6 shrink-0 items-center gap-1.5 rounded-full bg-attention-soft px-2 text-xs font-semibold text-attention-foreground">
          <span className="size-2 rounded-full bg-attention" aria-hidden />
          {needsYouLabel}
        </span>
      )}
      {branch && (
        <span className={cn(headerChipClassName, 'font-mono')} title={branch}>
          <span className="truncate">{branch}</span>
        </span>
      )}
      {modelLabel && (
        <span className={headerChipClassName}>
          <span className="truncate">{modelLabel}</span>
        </span>
      )}
      {actions}
    </header>
  );
}

interface WorkspacesMainProps {
  workspaceWithSession: WorkspacesMainWorkspace | undefined;
  isLoading: boolean;
  showLoadingOverlay?: boolean;
  containerRef: RefObject<HTMLElement>;
  /** Session header, rendered above the transcript */
  headerContent?: ReactNode;
  conversationContent?: ReactNode;
  chatBoxContent: ReactNode;
  isAtBottom?: boolean;
  onAtBottomChange?: (atBottom: boolean) => void;
  onScrollToBottom?: (behavior?: 'auto' | 'smooth') => void;
  isMobile?: boolean;
}

export function WorkspacesMain({
  workspaceWithSession,
  isLoading,
  showLoadingOverlay = false,
  containerRef,
  headerContent,
  conversationContent,
  chatBoxContent,
  isAtBottom = true,
  onScrollToBottom,
  isMobile,
}: WorkspacesMainProps) {
  const { t } = useTranslation(['tasks', 'common']);

  // Always render the main structure to prevent chat box flash during workspace transitions
  return (
    <main
      ref={containerRef}
      className={cn(
        'relative flex flex-1 flex-col bg-primary',
        isMobile ? 'min-h-0' : 'h-full'
      )}
    >
      {workspaceWithSession && !isLoading ? headerContent : null}
      {/* Conversation content - conditional based on loading/workspace state */}
      {isLoading ? (
        <div className="flex-1 flex items-center justify-center">
          <SpinnerIcon className="size-6 animate-spin text-low" />
        </div>
      ) : !workspaceWithSession ? (
        <div className="flex-1 flex items-center justify-center">
          <p className="text-sm text-low">
            {t('common:workspaces.selectToStart')}
          </p>
        </div>
      ) : (
        <>
          {showLoadingOverlay && (
            <div className="absolute inset-0 z-10 flex items-center justify-center bg-primary">
              <SpinnerIcon className="size-6 animate-spin text-low" />
            </div>
          )}
          {conversationContent}
        </>
      )}
      {/* Scroll to bottom button */}
      {workspaceWithSession && !isAtBottom && (
        <div className="flex justify-center pointer-events-none">
          <div className="w-chat max-w-full relative">
            <button
              type="button"
              onClick={() => onScrollToBottom?.('auto')}
              className={cn(
                'absolute bottom-2 right-6 z-10 pointer-events-auto flex size-8 items-center justify-center rounded-full border bg-secondary text-low shadow-overlay transition-colors hover:bg-panel hover:text-high',
                'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
              )}
              aria-label={t('common:workspaces.scrollToBottom')}
              title={t('common:workspaces.scrollToBottom')}
            >
              <ArrowDownIcon className="size-4" weight="bold" />
            </button>
          </div>
        </div>
      )}
      {/* Composer - always rendered to prevent flash during workspace switch */}
      <div
        className="flex justify-center border-t @container px-6 pb-[18px] pt-3"
        data-chatbox-container="true"
      >
        {chatBoxContent}
      </div>
    </main>
  );
}
