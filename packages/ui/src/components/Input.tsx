import * as React from 'react';
import { cn } from '../lib/cn';
import { twMerge } from 'tailwind-merge';

export interface InputProps
  extends React.InputHTMLAttributes<HTMLInputElement> {
  onCommandEnter?: (e: React.KeyboardEvent<HTMLInputElement>) => void;
  onCommandShiftEnter?: (e: React.KeyboardEvent<HTMLInputElement>) => void;
}

const Input = React.forwardRef<HTMLInputElement, InputProps>(
  (
    {
      className,
      type,
      onKeyDown,
      onCommandEnter,
      onCommandShiftEnter,
      ...props
    },
    ref
  ) => {
    const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === 'Escape') {
        e.currentTarget.blur();
      }
      if (e.key === 'Enter' && !e.nativeEvent.isComposing) {
        if (e.metaKey && e.shiftKey) {
          onCommandShiftEnter?.(e);
        } else {
          onCommandEnter?.(e);
        }
      }
      onKeyDown?.(e);
    };

    return (
      <input
        ref={ref}
        type={type}
        onKeyDown={handleKeyDown}
        className={twMerge(
          cn(
            'flex h-9 w-full rounded border border-border bg-secondary px-3 text-sm text-high placeholder:text-low file:border-0 file:bg-transparent file:text-sm file:font-medium focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand disabled:cursor-not-allowed disabled:opacity-50',
            className
          )
        )}
        {...props}
      />
    );
  }
);

Input.displayName = 'Input';
export { Input };
