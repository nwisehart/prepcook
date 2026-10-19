'use client';

import { Card } from './Card';
import { cn } from '../lib/cn';
import {
  DragDropContext,
  Droppable,
  Draggable,
  type DragStart,
  type DropResult,
  type DraggableProvided,
  type DraggableStateSnapshot,
  type DroppableProvided,
} from '@hello-pangea/dnd';
import {
  type KeyboardEvent,
  type MouseEvent,
  type MutableRefObject,
  type ReactNode,
  type Ref,
} from 'react';
import {
  CaretLeftIcon,
  ClockIcon,
  DotsSixVerticalIcon,
  MagnifyingGlassIcon,
} from '@phosphor-icons/react';

export type { DragStart, DropResult } from '@hello-pangea/dnd';

export type Status = {
  id: string;
  name: string;
  color: string;
};

export type Feature = {
  id: string;
  name: string;
  startAt: Date;
  endAt: Date;
  status: Status;
};

// =============================================================================
// Kanban Board (Column)
// =============================================================================

export type KanbanBoardProps = {
  children: ReactNode;
  className?: string;
};

/** One 236px column on the page ground: header, then the card list. */
export const KanbanBoard = ({ children, className }: KanbanBoardProps) => {
  return (
    <div className={cn('flex w-[236px] shrink-0 flex-col gap-2.5', className)}>
      {children}
    </div>
  );
};

// =============================================================================
// Kanban Card (Draggable)
// =============================================================================

/**
 * `attention` is a Needs-you card (amber border; its content draws the
 * strip), `archived` a cleaned-up card on the muted raised surface.
 */
export type KanbanCardTone = 'default' | 'attention' | 'archived';

export type KanbanCardProps = Pick<Feature, 'id' | 'name'> & {
  index: number;
  children?: ReactNode;
  className?: string;
  onClick?: (e: MouseEvent<HTMLDivElement>) => void;
  tabIndex?: number;
  forwardedRef?: Ref<HTMLDivElement>;
  onKeyDown?: (e: KeyboardEvent) => void;
  isOpen?: boolean;
  isSelected?: boolean;
  dragDisabled?: boolean;
  isMobile?: boolean;
  tone?: KanbanCardTone;
};

export const KanbanCard = ({
  id,
  name,
  index,
  children,
  className,
  onClick,
  tabIndex,
  forwardedRef,
  onKeyDown,
  isOpen,
  isSelected,
  dragDisabled = false,
  isMobile,
  tone = 'default',
}: KanbanCardProps) => {
  const fallback = (
    <p className="m-0 px-3.5 py-3 text-sm font-semibold text-high">{name}</p>
  );

  return (
    <Draggable draggableId={id} index={index} isDragDisabled={dragDisabled}>
      {(provided: DraggableProvided, snapshot: DraggableStateSnapshot) => {
        // Combine DnD ref and forwarded ref
        const setRefs = (node: HTMLDivElement | null) => {
          provided.innerRef(node);
          if (typeof forwardedRef === 'function') {
            forwardedRef(node);
          } else if (forwardedRef && typeof forwardedRef === 'object') {
            (forwardedRef as MutableRefObject<HTMLDivElement | null>).current =
              node;
          }
        };

        return (
          // The 12px gap between cards is a bottom margin rather than a
          // flex gap: the drag library measures margins, not gaps.
          <div
            ref={setRefs}
            {...provided.draggableProps}
            {...(isMobile ? {} : provided.dragHandleProps)}
            className="mb-3 outline-none"
            tabIndex={tabIndex}
            onClick={
              isMobile
                ? (e) => {
                    if (!snapshot.isDragging) onClick?.(e);
                  }
                : undefined
            }
            onMouseUp={
              !isMobile
                ? (e) => {
                    if (e.button === 0 && !snapshot.isDragging) {
                      onClick?.(e);
                    }
                  }
                : undefined
            }
            onKeyDown={onKeyDown}
          >
            <Card
              className={cn(
                'flex flex-col overflow-hidden transition-colors',
                tone === 'attention'
                  ? 'border-attention-border hover:border-attention'
                  : 'hover:border-low/40',
                tone === 'archived' && 'bg-surface-raised',
                isOpen && 'border-brand ring-2 ring-brand hover:border-brand',
                isSelected && 'border-brand ring-1 ring-brand bg-brand-soft/40',
                snapshot.isDragging && 'cursor-grabbing shadow-overlay',
                className
              )}
            >
              {isMobile ? (
                <div className="flex">
                  <div
                    {...provided.dragHandleProps}
                    className="flex shrink-0 cursor-grab items-start pl-2 pt-3"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <DotsSixVerticalIcon
                      className="size-icon-xs text-low"
                      weight="bold"
                    />
                  </div>
                  <div className="min-w-0 flex-1">{children ?? fallback}</div>
                </div>
              ) : (
                (children ?? fallback)
              )}
            </Card>
          </div>
        );
      }}
    </Draggable>
  );
};

// =============================================================================
// Kanban Cards Container
// =============================================================================

export type KanbanCardsProps = {
  id: string;
  children: ReactNode;
  className?: string;
  isDropDisabled?: boolean;
};

export const KanbanCards = ({
  id,
  children,
  className,
  isDropDisabled,
}: KanbanCardsProps) => (
  <Droppable droppableId={id} isDropDisabled={isDropDisabled}>
    {(provided: DroppableProvided) => (
      <div
        className={cn('flex min-h-24 flex-1 flex-col', className)}
        ref={provided.innerRef}
        {...provided.droppableProps}
      >
        {children}
        {provided.placeholder}
      </div>
    )}
  </Droppable>
);

// =============================================================================
// Kanban Header
// =============================================================================

export type KanbanHeaderProps =
  | {
      children: ReactNode;
    }
  | {
      name: Status['name'];
      color: Status['color'];
      className?: string;
    };

export const KanbanHeader = (props: KanbanHeaderProps) => {
  if ('children' in props) {
    return props.children;
  }

  return (
    <KanbanColumnHeader
      name={props.name}
      dot={{ color: props.color }}
      className={props.className}
    />
  );
};

/**
 * The header dot: hollow grey for To Do, brand for the working stages,
 * green for Shipped (and an older board's Close), grey for Clean up; custom
 * columns keep their own color
 * (an HSL triplet, as statuses store it).
 */
export type KanbanColumnDot =
  | 'hollow'
  | 'brand'
  | 'success'
  | 'muted'
  | { color: string };

export function KanbanColumnDotMark({ dot }: { dot: KanbanColumnDot }) {
  if (typeof dot === 'object') {
    return (
      <span
        aria-hidden="true"
        className="size-2.5 shrink-0 rounded-full"
        style={{ backgroundColor: `hsl(${dot.color})` }}
      />
    );
  }
  return (
    <span
      aria-hidden="true"
      className={cn(
        'size-2.5 shrink-0 rounded-full',
        dot === 'hollow' && 'border-2 border-low/70',
        dot === 'brand' && 'bg-brand',
        dot === 'success' && 'bg-success',
        dot === 'muted' && 'bg-low/50'
      )}
    />
  );
}

export type KanbanColumnHeaderProps = {
  name: string;
  count?: number;
  /** One short line on what the stage is for. */
  helper?: string | null;
  dot: KanbanColumnDot;
  /** Right-aligned control on the name row (e.g. a fold button). */
  trailing?: ReactNode;
  className?: string;
};

/** Dot + name (700) + muted count, with the helper line beneath. */
export function KanbanColumnHeader({
  name,
  count,
  helper,
  dot,
  trailing,
  className,
}: KanbanColumnHeaderProps) {
  return (
    <div
      className={cn(
        'sticky top-0 z-20 flex flex-col gap-0.5 bg-primary px-0.5 pb-0.5',
        className
      )}
    >
      <div className="flex min-h-[26px] items-center gap-2 text-sm">
        <KanbanColumnDotMark dot={dot} />
        <span className="truncate font-bold text-high">{name}</span>
        {count !== undefined && <span className="text-low">{count}</span>}
        {trailing && <span className="ml-auto shrink-0">{trailing}</span>}
      </div>
      {helper && <span className="text-xs text-low">{helper}</span>}
    </div>
  );
}

// =============================================================================
// Folded column rail
// =============================================================================

export type KanbanFoldedColumnProps = {
  name: string;
  count: number;
  /** Accessible name for the rail, e.g. "Clean up: 14 cards archived. Show them". */
  label: string;
  onUnfold: () => void;
  className?: string;
};

/**
 * A 56px dashed rail standing in for a folded column: count badge, the
 * column name set vertically, and a clock mark. Clicking it unfolds the
 * column in place.
 */
export function KanbanFoldedColumn({
  name,
  count,
  label,
  onUnfold,
  className,
}: KanbanFoldedColumnProps) {
  return (
    <button
      type="button"
      onClick={onUnfold}
      aria-label={label}
      title={label}
      className={cn(
        'flex w-14 shrink-0 flex-col items-center gap-2.5 self-stretch rounded-lg border border-dashed border-low/40 bg-transparent py-3 text-low transition-colors',
        'hover:border-low/70 hover:bg-panel/60 hover:text-normal',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
        className
      )}
    >
      <span className="inline-flex h-[22px] min-w-[22px] items-center justify-center rounded-full border border-border bg-secondary px-[7px] text-xs font-semibold text-normal">
        {count}
      </span>
      <span className="text-[13px] font-bold tracking-[0.02em] [writing-mode:vertical-rl]">
        {name}
      </span>
      <ClockIcon className="mt-auto size-4" aria-hidden="true" />
    </button>
  );
}

/** The square button in an unfolded column's header that folds it back. */
export function KanbanColumnFoldButton({
  label,
  onFold,
}: {
  label: string;
  onFold: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onFold}
      aria-label={label}
      title={label}
      className="flex size-[26px] items-center justify-center rounded-sm border border-border bg-secondary text-low transition-colors hover:bg-panel hover:text-high focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand"
    >
      <CaretLeftIcon className="size-3.5" weight="bold" />
    </button>
  );
}

/** A compact search box under a column header. */
export function KanbanColumnSearch({
  value,
  onChange,
  placeholder,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
}) {
  return (
    <label className="flex h-8 items-center gap-2 rounded border border-border bg-secondary px-2.5 text-low focus-within:border-brand">
      <MagnifyingGlassIcon className="size-3.5 shrink-0" aria-hidden="true" />
      <input
        type="search"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        aria-label={placeholder}
        className="min-w-0 flex-1 bg-transparent text-[13px] text-high outline-none placeholder:text-low"
      />
    </label>
  );
}

/** "This week" / "Earlier": an 11px section label inside a column. */
export function KanbanColumnGroupLabel({ children }: { children: ReactNode }) {
  return (
    <span className="px-0.5 pb-2 pt-1 text-[11px] font-bold uppercase tracking-[0.08em] text-low">
      {children}
    </span>
  );
}

// =============================================================================
// Kanban Provider (DragDropContext)
// =============================================================================

export type KanbanProviderProps = {
  children: ReactNode;
  onDragEnd: (result: DropResult) => void;
  onDragStart?: (start: DragStart) => void;
  className?: string;
};

export const KanbanProvider = ({
  children,
  onDragEnd,
  onDragStart,
  className,
}: KanbanProviderProps) => {
  return (
    <DragDropContext onDragEnd={onDragEnd} onDragStart={onDragStart}>
      <div className={cn('flex min-h-full items-stretch gap-3', className)}>
        {children}
      </div>
    </DragDropContext>
  );
};
