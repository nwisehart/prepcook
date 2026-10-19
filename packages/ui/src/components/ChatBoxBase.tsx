import { type ReactNode } from 'react';
import { ImageIcon } from '@phosphor-icons/react';
import { useTranslation } from 'react-i18next';
import { cn } from '../lib/cn';
import { Toolbar } from './Toolbar';

export enum VisualVariant {
  NORMAL = 'NORMAL',
  FEEDBACK = 'FEEDBACK',
  EDIT = 'EDIT',
  PLAN = 'PLAN',
}

export interface DropzoneProps {
  getRootProps: () => Record<string, unknown>;
  getInputProps: () => Record<string, unknown>;
  isDragActive: boolean;
}

interface ChatBoxBaseProps {
  // Editor node (provided by frontend)
  editor: ReactNode;

  // Error display
  error?: string | null;

  // Meta row, right side (session dropdown, todos, context gauge)
  headerRight?: ReactNode;

  // Meta row, left side (stats, in-progress todo)
  headerLeft?: ReactNode;

  // Meta row tools (attach, PR comments, toolbar actions)
  footerLeft?: ReactNode;

  // Action buttons beside the input (Send, Stop, Approve, ...)
  footerRight: ReactNode;

  // Model selector node, rendered as 40px secondary buttons beside the input
  modelSelector?: ReactNode;

  // Banner content (queued message indicator, question banner)
  banner?: ReactNode;

  // visualVariant
  visualVariant: VisualVariant;

  // Whether the agent is working (animated brand border on the input)
  isRunning?: boolean;

  // Dropzone props for drag-and-drop image uploads
  dropzone?: DropzoneProps;
}

/**
 * Base composer layout shared by CreateChatBox and SessionChatBox: a quiet
 * meta row (tools, stats, session controls), then the input with the model
 * selector and the action buttons beside it.
 */
export function ChatBoxBase({
  editor,
  error,
  headerRight,
  headerLeft,
  footerLeft,
  footerRight,
  modelSelector,
  banner,
  visualVariant,
  isRunning,
  dropzone,
}: ChatBoxBaseProps) {
  const { t } = useTranslation(['common', 'tasks']);

  const isDragActive = dropzone?.isDragActive ?? false;
  const isNormal = visualVariant === VisualVariant.NORMAL;
  const showMetaRow = Boolean(
    footerLeft || (isNormal && (headerLeft || headerRight))
  );

  return (
    <div
      {...(dropzone?.getRootProps() ?? {})}
      className="relative flex w-chat max-w-full flex-col gap-2"
    >
      {dropzone && <input {...dropzone.getInputProps()} />}

      {isDragActive && (
        <div className="pointer-events-none absolute inset-0 z-50 flex items-center justify-center rounded-lg border border-dashed border-brand bg-primary/90 animate-in fade-in-0 duration-150">
          <div className="text-center">
            <div className="mx-auto mb-2 flex size-10 items-center justify-center rounded-full bg-brand-soft">
              <ImageIcon className="size-5 text-brand" />
            </div>
            <p className="text-sm font-semibold text-high">
              {t('tasks:dropzone.dropImagesHere')}
            </p>
            <p className="mt-0.5 text-xs text-low">
              {t('tasks:dropzone.supportedFormats')}
            </p>
          </div>
        </div>
      )}

      {error && (
        <div
          role="alert"
          className="rounded bg-error-soft px-3 py-2 text-sm text-error"
        >
          {error}
        </div>
      )}

      {banner}

      {showMetaRow && (
        <div className="flex min-w-0 items-center gap-3 text-xs text-low">
          {footerLeft && (
            <Toolbar className="shrink-0 !gap-2">{footerLeft}</Toolbar>
          )}
          <div className="flex min-w-0 flex-1 items-center gap-2 overflow-hidden">
            {isNormal && headerLeft}
          </div>
          {isNormal && headerRight && (
            <Toolbar className="shrink-0 gap-2">{headerRight}</Toolbar>
          )}
        </div>
      )}

      <div className="flex flex-wrap items-end gap-2.5">
        <div
          className={cn(
            'relative flex min-h-10 min-w-0 flex-1 basis-[260px] items-center rounded-lg border bg-secondary px-3.5 py-2 transition-colors',
            'focus-within:border-brand',
            visualVariant === VisualVariant.PLAN && 'border-attention-border',
            (visualVariant === VisualVariant.FEEDBACK ||
              visualVariant === VisualVariant.EDIT) &&
              'border-brand',
            // The running border is drawn on ::before; match its radius to the
            // field and close it along the bottom edge.
            isRunning && 'chat-box-running before:rounded-lg before:pb-px'
          )}
        >
          <div className="min-w-0 flex-1 text-sm text-high">{editor}</div>
        </div>
        {modelSelector && (
          <div className="flex shrink-0 items-center gap-1.5 [&>button]:h-10 [&>button]:rounded [&>button]:font-semibold">
            {modelSelector}
          </div>
        )}
        <div className="flex shrink-0 items-center gap-2 [&>button]:min-h-10 [&>button]:px-4">
          {footerRight}
        </div>
      </div>
    </div>
  );
}
