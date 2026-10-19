import { cn } from '../lib/cn';
import { GitPullRequestIcon } from '@phosphor-icons/react';

export type PrBadgeStatus = 'open' | 'merged' | 'closed';

export interface PrBadgeProps {
  number: number;
  url: string;
  status: PrBadgeStatus;
  className?: string;
}

export function PrBadge({ number, url, status, className }: PrBadgeProps) {
  return (
    <a
      href={url}
      target="_blank"
      rel="noopener noreferrer"
      onClick={(e) => e.stopPropagation()}
      className={cn(
        'inline-flex h-[22px] items-center gap-1 rounded-full px-2 text-xs font-medium leading-none transition-colors',
        status === 'merged'
          ? 'bg-success-soft text-success hover:opacity-80'
          : status === 'closed'
            ? 'bg-panel text-low hover:text-normal'
            : 'bg-brand-soft text-brand-soft-foreground hover:opacity-80',
        className
      )}
    >
      <GitPullRequestIcon className="size-3" weight="bold" />
      <span className="font-mono">#{number}</span>
    </a>
  );
}
