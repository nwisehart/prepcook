import type { ButtonHTMLAttributes, ReactNode } from 'react';
import type { Icon } from '@phosphor-icons/react';
import {
  Layout as LayoutIcon,
  ChatsTeardrop as ChatsTeardropIcon,
  GitDiff as GitDiffIcon,
  Terminal as TerminalIcon,
  Desktop as DesktopIcon,
  GitFork as GitForkIcon,
  List as ListIcon,
  Gear as GearIcon,
  Kanban as KanbanIcon,
  CaretLeft as CaretLeftIcon,
  ArrowClockwise as ArrowClockwiseIcon,
  SidebarSimple as SidebarSimpleIcon,
} from '@phosphor-icons/react';
import { useTranslation } from 'react-i18next';
import { cn } from '../lib/cn';
import { Tooltip } from './Tooltip';
import {
  SyncErrorIndicator,
  type SyncErrorIndicatorError,
} from './SyncErrorIndicator';

/**
 * Action item rendered in the navbar.
 */
export interface NavbarActionItem {
  type?: 'action';
  id: string;
  icon: Icon;
  isActive?: boolean;
  tooltip?: string;
  shortcut?: string;
  disabled?: boolean;
  onClick?: () => void;
}

/**
 * Divider item rendered in the navbar.
 */
export interface NavbarDividerItem {
  type: 'divider';
}

export type NavbarSectionItem = NavbarActionItem | NavbarDividerItem;

function isDivider(item: NavbarSectionItem): item is NavbarDividerItem {
  return item.type === 'divider';
}

/** 32px ghost icon button; a selected toggle sits on the `bg-panel` fill. */
export const navbarIconButtonClassName = cn(
  'flex size-8 shrink-0 items-center justify-center rounded',
  'text-low transition-colors hover:bg-panel hover:text-high',
  'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
  'disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-low'
);

interface NavbarIconButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement> {
  icon: Icon;
  isActive?: boolean;
  tooltip?: string;
  shortcut?: string;
}

function NavbarIconButton({
  icon: IconComponent,
  isActive = false,
  tooltip,
  shortcut,
  className,
  ...props
}: NavbarIconButtonProps) {
  const button = (
    <button
      type="button"
      aria-pressed={isActive || undefined}
      className={cn(
        navbarIconButtonClassName,
        isActive && 'bg-panel text-high',
        className
      )}
      {...props}
    >
      <IconComponent className="size-4" />
    </button>
  );

  return tooltip ? (
    <Tooltip content={tooltip} shortcut={shortcut}>
      {button}
    </Tooltip>
  ) : (
    button
  );
}

export type MobileTabId =
  | 'workspaces'
  | 'chat'
  | 'review'
  | 'logs'
  | 'preview'
  | 'git';

export const MOBILE_TABS: { id: MobileTabId; icon: Icon; label: string }[] = [
  { id: 'workspaces', icon: LayoutIcon, label: 'Wksps' },
  { id: 'chat', icon: ChatsTeardropIcon, label: 'Chat' },
  { id: 'review', icon: GitDiffIcon, label: 'Review' },
  { id: 'logs', icon: TerminalIcon, label: 'Logs' },
  { id: 'preview', icon: DesktopIcon, label: 'Preview' },
  { id: 'git', icon: GitForkIcon, label: 'Git' },
];

export interface NavbarBreadcrumbItem {
  label: string;
  onClick?: () => void;
  /** Card keys and branch names render in the mono face. */
  mono?: boolean;
}

interface NavbarBreadcrumbsProps {
  breadcrumbs: NavbarBreadcrumbItem[];
  className?: string;
}

/** Muted 12.5px trail: project / card key / session. */
function NavbarBreadcrumbs({ breadcrumbs, className }: NavbarBreadcrumbsProps) {
  const { t } = useTranslation('common');
  return (
    <ol
      aria-label={t('navbar.breadcrumb')}
      className={cn(
        'flex min-w-0 items-center gap-1.5 text-[12.5px] text-low',
        className
      )}
    >
      {breadcrumbs.map((crumb, index) => {
        const isLast = index === breadcrumbs.length - 1;
        return (
          <li key={index} className="flex min-w-0 items-center gap-1.5">
            {index > 0 && (
              <span aria-hidden="true" className="shrink-0">
                /
              </span>
            )}
            {crumb.onClick && !isLast ? (
              <button
                type="button"
                className={cn(
                  'truncate rounded-sm transition-colors hover:text-high cursor-pointer',
                  'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
                  crumb.mono && 'font-mono text-xs'
                )}
                onClick={crumb.onClick}
              >
                {crumb.label}
              </button>
            ) : (
              <span
                aria-current={isLast ? 'page' : undefined}
                className={cn(
                  'truncate',
                  isLast && 'text-normal',
                  crumb.mono && 'font-mono text-xs'
                )}
              >
                {crumb.label}
              </span>
            )}
          </li>
        );
      })}
    </ol>
  );
}

export interface NavbarProps {
  /** Shown as a single breadcrumb when there is no breadcrumb trail. */
  workspaceTitle?: string;
  breadcrumbs?: NavbarBreadcrumbItem[];
  // Items for left side of navbar, after the breadcrumbs
  leftItems?: NavbarSectionItem[];
  // Items for right side of navbar (with dividers inline)
  rightItems?: NavbarSectionItem[];
  // Optional additional content for left side (after the breadcrumbs)
  leftSlot?: ReactNode;
  // Sync errors shown in the right section
  syncErrors?: readonly SyncErrorIndicatorError[] | null;
  /** Account menu trigger, last on the right. */
  userSlot?: ReactNode;
  className?: string;
  // Mobile props
  mobileMode?: boolean;
  isOnProjectPage?: boolean;
  onOpenCommandBar?: () => void;
  onOpenSettings?: () => void;
  onNavigateToBoard?: (() => void) | null;
  onNavigateBack?: () => void;
  onReload?: () => void;
  onOpenDrawer?: () => void;
  isOnProjectSubRoute?: boolean;
  mobileActiveTab?: MobileTabId;
  onMobileTabChange?: (tab: MobileTabId) => void;
  mobileTabs?: { id: MobileTabId; icon: Icon; label: string }[];
  showMobileTabs?: boolean;
  mobileShowBack?: boolean;
}

export function Navbar({
  workspaceTitle,
  breadcrumbs,
  leftItems = [],
  rightItems = [],
  leftSlot,
  syncErrors,
  userSlot,
  className,
  mobileMode = false,
  isOnProjectPage = false,
  onOpenCommandBar,
  onOpenSettings,
  onNavigateToBoard,
  onNavigateBack,
  onReload,
  onOpenDrawer,
  isOnProjectSubRoute = false,
  mobileActiveTab = 'chat',
  onMobileTabChange,
  mobileTabs,
  showMobileTabs,
  mobileShowBack,
}: NavbarProps) {
  const { t } = useTranslation('common');
  const trail: NavbarBreadcrumbItem[] | null =
    breadcrumbs && breadcrumbs.length > 0
      ? breadcrumbs
      : workspaceTitle
        ? [{ label: workspaceTitle }]
        : null;

  const renderItem = (item: NavbarSectionItem, key: string) => {
    if (isDivider(item)) {
      return (
        <div key={key} aria-hidden="true" className="mx-1 h-4 w-px bg-border" />
      );
    }

    return (
      <NavbarIconButton
        key={key}
        icon={item.icon}
        isActive={item.isActive}
        onClick={item.onClick}
        aria-label={item.tooltip}
        tooltip={item.tooltip}
        shortcut={item.shortcut}
        disabled={!!item.disabled}
      />
    );
  };

  // ---- Mobile layout ----
  if (mobileMode) {
    return (
      <nav
        className={cn(
          'flex shrink-0 flex-col border-b border-border bg-primary',
          className
        )}
      >
        {/* Row 1: Tab bar (workspace pages) or minimal header (project pages) */}
        <div className="flex h-11 items-center justify-between gap-2 px-2">
          {isOnProjectPage ? (
            <div className="flex min-w-0 items-center gap-1">
              {isOnProjectSubRoute
                ? onNavigateBack && (
                    <button
                      type="button"
                      className={navbarIconButtonClassName}
                      onClick={onNavigateBack}
                      aria-label="Back"
                    >
                      <CaretLeftIcon className="size-4" />
                    </button>
                  )
                : onOpenDrawer && (
                    <button
                      type="button"
                      className={navbarIconButtonClassName}
                      onClick={onOpenDrawer}
                      aria-label="Open menu"
                    >
                      <SidebarSimpleIcon className="size-4" />
                    </button>
                  )}
              {trail && (
                <NavbarBreadcrumbs
                  breadcrumbs={trail}
                  className="select-none"
                />
              )}
            </div>
          ) : (
            <div className="flex min-w-0 items-center gap-0.5 overflow-x-auto">
              {mobileShowBack && onNavigateBack ? (
                <>
                  <button
                    type="button"
                    className={navbarIconButtonClassName}
                    onClick={onNavigateBack}
                    aria-label="Back"
                  >
                    <CaretLeftIcon className="size-4" />
                  </button>
                  <div className="mx-0.5 h-4 w-px shrink-0 bg-border" />
                </>
              ) : (
                onOpenDrawer && (
                  <>
                    <button
                      type="button"
                      className={navbarIconButtonClassName}
                      onClick={onOpenDrawer}
                      aria-label="Projects"
                    >
                      <KanbanIcon className="size-4" />
                    </button>
                    <div className="mx-0.5 h-4 w-px shrink-0 bg-border" />
                  </>
                )
              )}
              {showMobileTabs !== false &&
                (mobileTabs ?? MOBILE_TABS).map((tab) => {
                  const TabIcon = tab.icon;
                  const isActive = mobileActiveTab === tab.id;
                  return (
                    <button
                      key={tab.id}
                      type="button"
                      aria-pressed={isActive}
                      className={cn(
                        'flex h-8 items-center gap-1 whitespace-nowrap rounded px-2 text-xs font-medium transition-colors',
                        isActive
                          ? 'bg-panel text-high'
                          : 'text-low hover:bg-panel hover:text-high'
                      )}
                      onClick={() => onMobileTabChange?.(tab.id)}
                    >
                      <TabIcon className="size-4" />
                      <span className="hidden min-[480px]:inline">
                        {tab.label}
                      </span>
                    </button>
                  );
                })}
              {onNavigateToBoard && (
                <button
                  type="button"
                  className="flex h-8 items-center gap-1 whitespace-nowrap rounded px-2 text-xs font-medium text-low transition-colors hover:bg-panel hover:text-high"
                  onClick={onNavigateToBoard}
                >
                  <KanbanIcon className="size-4" />
                  <span className="hidden min-[480px]:inline">
                    {t('appBar.nav.board')}
                  </span>
                </button>
              )}
            </div>
          )}

          {/* Right side: sync indicator + action buttons + user slot */}
          <div className="flex shrink-0 items-center gap-0.5">
            <SyncErrorIndicator errors={syncErrors} />
            {isOnProjectPage &&
              rightItems
                .filter((item): item is NavbarActionItem => !isDivider(item))
                .map((item) => (
                  <NavbarIconButton
                    key={item.id}
                    icon={item.icon}
                    isActive={item.isActive}
                    onClick={item.onClick}
                    aria-label={item.tooltip}
                    tooltip={item.tooltip}
                    disabled={!!item.disabled}
                  />
                ))}
            {onReload && (
              <button
                type="button"
                className={navbarIconButtonClassName}
                onClick={onReload}
                aria-label="Reload"
              >
                <ArrowClockwiseIcon className="size-4" />
              </button>
            )}
            {!isOnProjectPage && onOpenSettings && (
              <button
                type="button"
                className={navbarIconButtonClassName}
                onClick={onOpenSettings}
                aria-label="Settings"
              >
                <GearIcon className="size-4" />
              </button>
            )}
            {!isOnProjectPage && onOpenCommandBar && (
              <button
                type="button"
                className={navbarIconButtonClassName}
                onClick={onOpenCommandBar}
                aria-label="Command bar"
              >
                <ListIcon className="size-4" />
              </button>
            )}
            {userSlot && <div className="mx-0.5 h-4 w-px shrink-0 bg-border" />}
            {userSlot}
          </div>
        </div>

        {/* Row 2: leftSlot + breadcrumbs (workspace pages only) */}
        {!isOnProjectPage && trail && (
          <div className="flex items-center gap-2 border-t border-border px-3 py-1.5">
            {leftSlot}
            <NavbarBreadcrumbs breadcrumbs={trail} className="flex-1" />
          </div>
        )}
      </nav>
    );
  }

  // ---- Desktop layout ----
  // data-tauri-drag-region must be on every non-interactive element for Tauri 2
  // window dragging to work (the attribute does not propagate to children).
  return (
    <nav
      data-tauri-drag-region
      className={cn(
        'flex h-11 shrink-0 items-center gap-3 border-b border-border bg-primary pl-5 pr-3',
        className
      )}
    >
      {/* Left - breadcrumbs, then page-scoped items */}
      <div
        data-tauri-drag-region
        className="flex min-w-0 flex-1 items-center gap-2"
      >
        {trail && (
          <NavbarBreadcrumbs
            breadcrumbs={trail}
            className="cursor-default select-none"
          />
        )}
        {leftSlot}
        {leftItems.map((item, index) =>
          renderItem(
            item,
            `left-${isDivider(item) ? 'divider' : item.id}-${index}`
          )
        )}
      </div>

      {/* Right - sync errors, panel toggles and global actions, account */}
      <div
        data-tauri-drag-region
        className="flex shrink-0 items-center gap-0.5"
      >
        <SyncErrorIndicator errors={syncErrors} />
        {rightItems.map((item, index) =>
          renderItem(
            item,
            `right-${isDivider(item) ? 'divider' : item.id}-${index}`
          )
        )}
        {userSlot}
      </div>
    </nav>
  );
}
