import type { ReactNode } from 'react';
import type { Icon } from '@phosphor-icons/react';
import { cn } from '../lib/cn';

interface AppBarButtonProps {
  icon?: Icon;
  label: string;
  isActive?: boolean;
  onClick?: () => void;
  /** Rendered dimmed and inert, e.g. Board while there is no project. */
  disabled?: boolean;
  /** Trailing content, e.g. a count chip. */
  badge?: ReactNode;
  className?: string;
  children?: ReactNode;
}

/**
 * One sidebar nav item: a 16px icon and a label. The active item sits on the
 * `bg-panel` fill with its icon in the brand color; inactive icons are muted.
 */
export function AppBarButton({
  icon: IconComponent,
  label,
  isActive = false,
  onClick,
  disabled = false,
  badge,
  className,
  children,
}: AppBarButtonProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-current={isActive ? 'page' : undefined}
      className={cn(
        'flex w-full items-center gap-2.5 rounded px-2.5 py-2 text-left',
        'text-sm font-medium text-high transition-colors',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
        isActive ? 'bg-panel' : 'hover:bg-panel',
        disabled
          ? 'cursor-not-allowed opacity-50 hover:bg-transparent'
          : 'cursor-pointer',
        className
      )}
    >
      {IconComponent && (
        <IconComponent
          className={cn(
            'size-4 shrink-0',
            isActive ? 'text-brand dark:text-brand-secondary' : 'text-low'
          )}
        />
      )}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {badge}
      {children}
    </button>
  );
}
