import { cn } from '../lib/cn';

export interface ToolStatusLike {
  status: string;
}

interface ToolStatusDotProps {
  status: ToolStatusLike;
  className?: string;
}

export function ToolStatusDot({ status, className }: ToolStatusDotProps) {
  const statusType = status.status;

  // Map status to visual state
  const isSuccess = statusType === 'success';
  const isError =
    statusType === 'failed' ||
    statusType === 'denied' ||
    statusType === 'timed_out';
  const isPending = statusType === 'created';
  const isWaiting = statusType === 'pending_approval';

  return (
    <span className={cn('inline-flex', className)}>
      <span
        className={cn(
          'size-1.5 rounded-full',
          isSuccess && 'bg-success',
          isError && 'bg-error',
          isPending && 'bg-brand',
          isWaiting && 'bg-attention'
        )}
      />
      {(isPending || isWaiting) && (
        <span
          className={cn(
            'absolute inset-0 size-1.5 rounded-full animate-ping',
            isPending ? 'bg-brand' : 'bg-attention'
          )}
        />
      )}
    </span>
  );
}
