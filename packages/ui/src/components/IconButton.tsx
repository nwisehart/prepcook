import { cn } from '../lib/cn';
import type { Icon } from '@phosphor-icons/react';

interface IconButtonProps {
  icon: Icon;
  iconClassName?: string;
  onClick?: () => void;
  disabled?: boolean;
  variant?: 'default' | 'tertiary';
  'aria-label': string;
  title?: string;
  className?: string;
}

export function IconButton({
  icon: IconComponent,
  iconClassName,
  onClick,
  disabled,
  variant = 'default',
  'aria-label': ariaLabel,
  title,
  className,
}: IconButtonProps) {
  const variantStyles = disabled
    ? 'opacity-40 cursor-not-allowed'
    : variant === 'default'
      ? 'text-low hover:text-high hover:bg-panel'
      : 'bg-panel hover:bg-border text-normal hover:text-high';

  return (
    <button
      type="button"
      className={cn(
        'flex items-center justify-center p-half rounded transition-colors',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
        variantStyles,
        className
      )}
      onClick={onClick}
      disabled={disabled}
      aria-label={ariaLabel}
      title={title}
    >
      <IconComponent className={cn('size-icon-sm', iconClassName)} />
    </button>
  );
}
