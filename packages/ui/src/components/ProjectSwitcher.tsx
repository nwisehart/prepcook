import {
  DragDropContext,
  Draggable,
  Droppable,
  type DropResult,
} from '@hello-pangea/dnd';
import {
  useId,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from 'react';
import {
  CaretDownIcon,
  CheckIcon,
  DownloadSimpleIcon,
  KanbanIcon,
  PlusIcon,
  SpinnerIcon,
  type Icon,
} from '@phosphor-icons/react';
import { useTranslation } from 'react-i18next';
import { cn } from '../lib/cn';

export interface AppBarProject {
  id: string;
  name: string;
  color: string;
}

interface ProjectSwitcherProps {
  projects: AppBarProject[];
  /** The project the switcher names, also checked in the list. */
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
}

function ProjectInitialTile({ children }: { children: ReactNode }) {
  return (
    <span
      aria-hidden="true"
      className="inline-flex size-[22px] shrink-0 items-center justify-center rounded-sm bg-brand-soft text-xs font-bold text-brand-soft-foreground"
    >
      {children}
    </span>
  );
}

function getProjectInitial(name: string): string {
  return name.trim().charAt(0).toUpperCase() || '?';
}

const listRowClassName = cn(
  'flex w-full items-center gap-2.5 rounded-sm px-2 py-1.5 text-left',
  'text-sm text-high transition-colors cursor-pointer',
  'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
);

function ListAction({
  icon: IconComponent,
  label,
  isActive = false,
  onClick,
}: {
  icon: Icon;
  label: string;
  isActive?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={isActive ? 'page' : undefined}
      className={cn(listRowClassName, isActive ? 'bg-panel' : 'hover:bg-panel')}
    >
      <span className="inline-flex size-[22px] shrink-0 items-center justify-center">
        <IconComponent className="size-4 text-low" />
      </span>
      <span className="min-w-0 flex-1 truncate">{label}</span>
    </button>
  );
}

/**
 * Bordered button naming the current project. It unfolds an inline list
 * underneath (not a popover) so the drag-to-reorder list keeps working: the
 * drag library positions the dragged row with `position: fixed`, which a
 * transformed popover would offset.
 */
export function ProjectSwitcher({
  projects,
  currentProjectId,
  onProjectClick,
  onProjectsDragEnd,
  isSavingProjectOrder = false,
  onCreateProject,
  hasBoardAccess = false,
  isLoadingProjects = false,
  onOpenBoardSettings,
  onExportClick,
  isExportActive = false,
}: ProjectSwitcherProps) {
  const { t } = useTranslation('common');
  const [isOpen, setIsOpen] = useState(false);
  const listId = useId();
  const triggerRef = useRef<HTMLButtonElement>(null);
  const currentProject =
    projects.find((project) => project.id === currentProjectId) ?? null;

  const close = () => setIsOpen(false);
  const handlePanelKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'Escape') {
      event.stopPropagation();
      close();
      triggerRef.current?.focus();
    }
  };

  const label = !hasBoardAccess
    ? t('kanban.boardRequired.title')
    : (currentProject?.name ??
      (projects.length > 0
        ? t('appBar.projects.placeholder')
        : t('appBar.projects.empty')));

  return (
    <div className="flex flex-col gap-1">
      <button
        ref={triggerRef}
        type="button"
        aria-expanded={isOpen}
        aria-controls={listId}
        onClick={() => setIsOpen((open) => !open)}
        className={cn(
          'flex w-full items-center gap-2.5 rounded border bg-secondary px-2.5 py-2 text-left',
          'text-sm text-high transition-colors cursor-pointer hover:bg-panel',
          'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
        )}
      >
        <ProjectInitialTile>
          {hasBoardAccess && currentProject ? (
            getProjectInitial(currentProject.name)
          ) : (
            <KanbanIcon className="size-3.5" />
          )}
        </ProjectInitialTile>
        <span className="min-w-0 flex-1 truncate font-semibold">{label}</span>
        {isLoadingProjects ? (
          <SpinnerIcon className="size-4 shrink-0 animate-spin text-low" />
        ) : (
          <CaretDownIcon
            className={cn(
              'size-4 shrink-0 text-low transition-transform',
              isOpen && 'rotate-180'
            )}
          />
        )}
      </button>

      {isOpen && (
        <div
          id={listId}
          onKeyDown={handlePanelKeyDown}
          className="flex flex-col rounded border bg-secondary p-1"
        >
          {!hasBoardAccess ? (
            <div className="flex flex-col gap-2 p-2">
              <p className="text-xs text-low">
                {t('kanban.boardRequired.description')}
              </p>
              {onOpenBoardSettings && (
                <button
                  type="button"
                  onClick={() => {
                    close();
                    onOpenBoardSettings();
                  }}
                  className="h-8 rounded bg-brand px-3 text-sm font-semibold text-on-brand transition-colors hover:bg-brand-hover cursor-pointer focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand-secondary"
                >
                  {t('kanban.boardRequired.action')}
                </button>
              )}
            </div>
          ) : (
            <>
              {projects.length === 0 && isLoadingProjects && (
                <div className="flex items-center justify-center py-2">
                  <SpinnerIcon className="size-4 animate-spin text-low" />
                </div>
              )}
              {projects.length === 0 && !isLoadingProjects && (
                <p className="px-2 py-1.5 text-xs text-low">
                  {t('appBar.projects.empty')}
                </p>
              )}
              {projects.length > 0 && (
                <DragDropContext onDragEnd={onProjectsDragEnd}>
                  <Droppable
                    droppableId="app-bar-projects"
                    direction="vertical"
                    isDropDisabled={isSavingProjectOrder}
                  >
                    {(dropProvided) => (
                      <ul
                        ref={dropProvided.innerRef}
                        {...dropProvided.droppableProps}
                        aria-label={t('appBar.projects.label')}
                        className="flex flex-col"
                      >
                        {projects.map((project, index) => {
                          const isCurrent = project.id === currentProjectId;
                          return (
                            <Draggable
                              key={project.id}
                              draggableId={project.id}
                              index={index}
                              disableInteractiveElementBlocking
                              isDragDisabled={isSavingProjectOrder}
                            >
                              {(dragProvided, snapshot) => (
                                <li
                                  ref={dragProvided.innerRef}
                                  {...dragProvided.draggableProps}
                                  {...dragProvided.dragHandleProps}
                                  style={dragProvided.draggableProps.style}
                                  className="rounded-sm py-px"
                                >
                                  <button
                                    type="button"
                                    onClick={() => {
                                      close();
                                      onProjectClick(project.id);
                                    }}
                                    aria-current={
                                      isCurrent ? 'true' : undefined
                                    }
                                    className={cn(
                                      listRowClassName,
                                      isCurrent
                                        ? 'bg-panel font-semibold'
                                        : 'hover:bg-panel',
                                      snapshot.isDragging &&
                                        'cursor-grabbing bg-secondary shadow-overlay'
                                    )}
                                  >
                                    <ProjectInitialTile>
                                      {getProjectInitial(project.name)}
                                    </ProjectInitialTile>
                                    <span className="min-w-0 flex-1 truncate">
                                      {project.name}
                                    </span>
                                    {isCurrent && (
                                      <CheckIcon className="size-4 shrink-0 text-brand dark:text-brand-secondary" />
                                    )}
                                  </button>
                                </li>
                              )}
                            </Draggable>
                          );
                        })}
                        {dropProvided.placeholder}
                      </ul>
                    )}
                  </Droppable>
                </DragDropContext>
              )}
              {(onCreateProject || onExportClick) && (
                <div className="mx-1 my-1 h-px bg-divider" />
              )}
              {onCreateProject && (
                <ListAction
                  icon={PlusIcon}
                  label={t('appBar.projects.create')}
                  onClick={() => {
                    close();
                    onCreateProject();
                  }}
                />
              )}
              {onExportClick && (
                <ListAction
                  icon={DownloadSimpleIcon}
                  label={t('appBar.projects.export')}
                  isActive={isExportActive}
                  onClick={() => {
                    close();
                    onExportClick();
                  }}
                />
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}
