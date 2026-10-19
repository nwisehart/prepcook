import type { ReactNode } from 'react';
import { cn } from '../lib/cn';

export type CommentCardVariant = 'user' | 'github' | 'input';

interface CommentCardProps {
  /** Determines the visual styling */
  variant: CommentCardVariant;
  /** Main content (editor, text, etc.) */
  children: ReactNode;
  /** Optional header (author, timestamp) */
  header?: ReactNode;
  /** Optional action buttons */
  actions?: ReactNode;
  /** Additional className for the outer wrapper */
  className?: string;
}

const variantStyles: Record<CommentCardVariant, string> = {
  user: 'bg-brand-soft border-brand/40',
  github: 'bg-secondary border-border',
  input: 'bg-brand-soft border-brand/40',
};

/**
 * Shared primitive for displaying comments in diff views.
 * Used by ReviewCommentRenderer, GitHubCommentRenderer, and CommentWidgetLine.
 */
export function CommentCard({
  variant,
  children,
  header,
  actions,
  className,
}: CommentCardProps) {
  return (
    <div className="p-base bg-panel font-sans text-sm">
      <div
        className={cn(
          'rounded border px-3 py-2.5',
          variantStyles[variant],
          className
        )}
      >
        {header && <div className="mb-half">{header}</div>}
        {children}
        {actions && <div className="mt-half flex gap-half">{actions}</div>}
      </div>
    </div>
  );
}
