import type { ReactNode } from 'react';
import { cn } from '../lib/cn';
import type { Icon } from '@phosphor-icons/react';
import { Tooltip } from './Tooltip';

interface ButtonGroupProps {
  children: ReactNode;
  className?: string;
}

/**
 * ButtonGroup - A segmented control: a bordered surface (8px radius, 3px
 * inset) holding IconButtonGroupItem (icon-only) or ButtonGroupItem
 * (text/mixed) segments with 6px radius.
 */
export function ButtonGroup({ children, className }: ButtonGroupProps) {
  return (
    <div
      className={cn(
        'flex items-center gap-0.5 rounded border border-border bg-secondary p-[3px]',
        className
      )}
    >
      {children}
    </div>
  );
}

// Alias for backwards compatibility
export const IconButtonGroup = ButtonGroup;

interface IconButtonGroupItemProps {
  icon: Icon;
  iconClassName?: string;
  onClick?: () => void;
  disabled?: boolean;
  active?: boolean;
  'aria-label': string;
  title?: string;
  className?: string;
}

export function IconButtonGroupItem({
  icon: IconComponent,
  iconClassName,
  onClick,
  disabled,
  active,
  'aria-label': ariaLabel,
  title,
  className,
}: IconButtonGroupItemProps) {
  const stateStyles = disabled
    ? 'text-low opacity-40 cursor-not-allowed'
    : active
      ? 'bg-panel text-high'
      : 'text-low hover:text-high';

  const button = (
    <button
      type="button"
      className={cn(
        'inline-flex size-7 items-center justify-center rounded-sm transition-colors',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
        stateStyles,
        className
      )}
      onClick={onClick}
      disabled={disabled}
      aria-label={ariaLabel}
    >
      <IconComponent className={cn('size-icon-sm', iconClassName)} />
    </button>
  );

  return title ? <Tooltip content={title}>{button}</Tooltip> : button;
}

interface ButtonGroupItemProps {
  icon?: Icon;
  iconClassName?: string;
  onClick?: () => void;
  disabled?: boolean;
  active?: boolean;
  'aria-label'?: string;
  title?: string;
  className?: string;
  children?: ReactNode;
}

/**
 * ButtonGroupItem - A button within a ButtonGroup that supports text, icons, or both
 */
export function ButtonGroupItem({
  icon: IconComponent,
  iconClassName,
  onClick,
  disabled,
  active,
  'aria-label': ariaLabel,
  title,
  className,
  children,
}: ButtonGroupItemProps) {
  const stateStyles = disabled
    ? 'text-low font-medium opacity-40 cursor-not-allowed'
    : active
      ? 'bg-panel text-high font-semibold'
      : 'text-low font-medium hover:text-high';

  // Square segment for icon-only, padded segment for text content
  const paddingStyles = children ? 'px-3' : 'w-7 justify-center';

  const button = (
    <button
      type="button"
      className={cn(
        'inline-flex h-7 items-center rounded-sm text-sm transition-colors',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
        paddingStyles,
        stateStyles,
        className
      )}
      onClick={onClick}
      disabled={disabled}
      aria-label={ariaLabel}
    >
      {IconComponent && (
        <IconComponent
          className={cn('size-icon-sm', children && 'mr-half', iconClassName)}
        />
      )}
      {children}
    </button>
  );

  return title ? <Tooltip content={title}>{button}</Tooltip> : button;
}
