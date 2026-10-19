import { WarningIcon, ArrowClockwiseIcon } from '@phosphor-icons/react';
import { useTranslation } from 'react-i18next';
import { Popover, PopoverTrigger, PopoverContent } from './Popover';

export interface SyncErrorIndicatorError {
  streamId: string;
  tableName: string;
  error: {
    message: string;
    status?: string | number | null;
  };
}

export interface SyncErrorIndicatorProps {
  errors?: readonly SyncErrorIndicatorError[] | null;
  onRefreshPage?: () => void;
}

/**
 * Displays a warning indicator when there are sync errors.
 * Shows a popover with error details on click.
 * Returns null when there are no errors.
 */
export function SyncErrorIndicator({
  errors,
  onRefreshPage,
}: SyncErrorIndicatorProps) {
  const { t } = useTranslation('common');

  if (!errors || errors.length === 0) {
    return null;
  }

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          className="flex size-8 shrink-0 items-center justify-center rounded text-error transition-colors hover:bg-error-soft data-[state=open]:bg-error-soft focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand"
          aria-label={`${errors.length} sync error${errors.length > 1 ? 's' : ''}`}
        >
          <WarningIcon className="size-4" weight="fill" />
        </button>
      </PopoverTrigger>
      <PopoverContent side="bottom" align="end" className="w-80">
        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between gap-2">
            <h4 className="text-sm font-bold text-high">
              {t('syncError.networkErrors')}
            </h4>
            <span className="text-xs text-low">
              {t('syncError.streamsAffected', { count: errors.length })}
            </span>
          </div>

          <div className="flex max-h-48 flex-col gap-1 overflow-y-auto">
            {errors.map((streamError) => (
              <div
                key={streamError.streamId}
                className="rounded-sm bg-error-soft px-2 py-1.5 text-xs"
              >
                <div className="font-semibold text-error">
                  {streamError.tableName}
                </div>
                <div className="mt-0.5 truncate text-normal">
                  {streamError.error.message}
                  {streamError.error.status && (
                    <span className="ml-1 text-low">
                      {t('syncError.status', {
                        status: streamError.error.status,
                      })}
                    </span>
                  )}
                </div>
              </div>
            ))}
          </div>

          <button
            type="button"
            onClick={() =>
              onRefreshPage ? onRefreshPage() : window.location.reload()
            }
            className="flex h-8 w-full items-center justify-center gap-1.5 rounded border bg-secondary px-3 text-sm font-semibold text-high transition-colors hover:bg-panel focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand"
          >
            <ArrowClockwiseIcon className="size-4" />
            {t('syncError.refreshPage')}
          </button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
