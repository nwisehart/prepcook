import * as React from 'react';
import { Check } from 'lucide-react';
import { cn } from '../lib/cn';

interface CheckboxProps {
  id?: string;
  checked?: boolean;
  onCheckedChange?: (checked: boolean) => void;
  className?: string;
  disabled?: boolean;
}

const Checkbox = React.forwardRef<HTMLButtonElement, CheckboxProps>(
  (
    { className, checked = false, onCheckedChange, disabled, ...props },
    ref
  ) => {
    return (
      <button
        type="button"
        role="checkbox"
        aria-checked={checked}
        ref={ref}
        className={cn(
          'peer flex size-4 shrink-0 items-center justify-center rounded-[0.25rem] border border-border bg-secondary transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand disabled:cursor-not-allowed disabled:opacity-50',
          checked && 'border-brand bg-brand text-on-brand',
          className
        )}
        disabled={disabled}
        onClick={() => onCheckedChange?.(!checked)}
        {...props}
      >
        {checked && (
          <div className="flex items-center justify-center text-current">
            <Check className="size-3" strokeWidth={3} />
          </div>
        )}
      </button>
    );
  }
);
Checkbox.displayName = 'Checkbox';

export { Checkbox };
