import * as DropdownMenuPrimitive from '@radix-ui/react-dropdown-menu';
import { CaretDownIcon, CheckIcon, type Icon } from '@phosphor-icons/react';
import { cn } from '../lib/cn';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
} from './Dropdown';

export interface SplitButtonOption<T extends string> {
  value: T;
  label: string;
  icon?: Icon;
}

interface SplitButtonProps<T extends string = string> {
  options: SplitButtonOption<T>[];
  selectedValue: T;
  onSelectionChange: (value: T) => void;
  onAction: (value: T) => void;
  className?: string;
}

export function SplitButton<T extends string>({
  options,
  selectedValue,
  onSelectionChange,
  onAction,
  className,
}: SplitButtonProps<T>) {
  const selectedOption = options.find((opt) => opt.value === selectedValue);
  const label = selectedOption?.label ?? '';

  return (
    <div className={cn('flex', className)}>
      <div className="flex items-stretch">
        {/* Primary CTA button */}
        <button
          type="button"
          onClick={() => onAction(selectedValue)}
          className={cn(
            'flex-1 min-h-8 border border-border bg-secondary px-3 hover:bg-panel',
            'text-sm font-semibold text-high transition-colors',
            'focus:outline-none focus-visible:ring-1 focus-visible:ring-brand focus-visible:ring-inset',
            'rounded-l'
          )}
        >
          {label}
        </button>

        {/* Dropdown trigger */}
        <DropdownMenu>
          <DropdownMenuPrimitive.Trigger
            className={cn(
              'flex min-h-8 items-center justify-center border border-l-0 border-border bg-secondary px-2 hover:bg-panel transition-colors',
              'focus:outline-none focus-visible:ring-1 focus-visible:ring-brand focus-visible:ring-inset',
              'rounded-r'
            )}
          >
            <CaretDownIcon className="size-icon-xs text-low" />
          </DropdownMenuPrimitive.Trigger>
          <DropdownMenuContent align="end">
            {options.map((option) => (
              <DropdownMenuItem
                disabled={option.value === selectedValue}
                key={option.value}
                onClick={() => onSelectionChange(option.value)}
                icon={option.value === selectedValue ? CheckIcon : option.icon}
              >
                {option.label}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  );
}
