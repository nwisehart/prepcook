import * as React from 'react';
import { twMerge } from 'tailwind-merge';
import { cva, type VariantProps } from 'class-variance-authority';

import { cn } from '../lib/cn';

// Chips: 22px pill, 12px / 500. `default` is the brand-soft chip (story,
// counts), `secondary` the neutral chip (priority), `destructive` the bug chip.
const badgeVariants = cva(
  'inline-flex h-[22px] items-center gap-1.5 whitespace-nowrap rounded-full border px-2 text-xs font-medium leading-none transition-colors focus:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
  {
    variants: {
      variant: {
        default: 'border-transparent bg-brand-soft text-brand-soft-foreground',
        secondary: 'border-transparent bg-panel text-normal',
        destructive: 'border-transparent bg-error-soft text-error',
        outline: 'border-border text-normal',
      },
    },
    defaultVariants: {
      variant: 'default',
    },
  }
);

export interface BadgeProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof badgeVariants> {}

function Badge({ className, variant, ...props }: BadgeProps) {
  return (
    <div
      className={twMerge(cn(badgeVariants({ variant }), className))}
      {...props}
    />
  );
}

export { Badge, badgeVariants };
