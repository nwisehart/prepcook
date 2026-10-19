import { SpinnerIcon, type Icon } from '@phosphor-icons/react';
import { cn } from '../lib/cn';

interface PrimaryButtonProps {
  variant?: 'default' | 'secondary' | 'tertiary';
  actionIcon?: Icon | 'spinner';
  value?: string;
  onClick?: () => void;
  disabled?: boolean;
  children?: React.ReactNode;
  className?: string;
}

export function PrimaryButton({
  variant = 'default',
  actionIcon: ActionIcon,
  value,
  onClick,
  disabled,
  children,
  className,
}: PrimaryButtonProps) {
  // default = primary (brand fill), secondary = bordered surface,
  // tertiary = quiet filled button for inline property controls.
  const variantStyles = disabled
    ? 'cursor-not-allowed bg-panel text-low'
    : variant === 'default'
      ? 'bg-brand hover:bg-brand-hover text-on-brand'
      : variant === 'secondary'
        ? 'bg-secondary border border-border hover:bg-panel text-high'
        : 'bg-panel hover:bg-border text-normal hover:text-high';

  return (
    <button
      className={cn(
        'flex min-h-8 items-center gap-half rounded px-3 text-sm font-semibold transition-colors',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
        variantStyles,
        className
      )}
      onClick={onClick}
      disabled={disabled}
    >
      {value}
      {children}
      {ActionIcon ? (
        ActionIcon === 'spinner' ? (
          <SpinnerIcon className={'size-icon-sm animate-spin'} />
        ) : (
          <ActionIcon className={'size-icon-xs'} />
        )
      ) : null}
    </button>
  );
}
