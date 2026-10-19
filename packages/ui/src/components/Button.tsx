import * as React from 'react';
import { twMerge } from 'tailwind-merge';
import { Slot } from '@radix-ui/react-slot';
import { cva, type VariantProps } from 'class-variance-authority';

import { cn } from '../lib/cn';

// 32px tall, 8px radius, 600 weight. `default` is the primary (brand fill)
// action; `outline` and `secondary` are the bordered surface button; `ghost`
// is transparent until hovered.
const buttonVariants = cva(
  'inline-flex items-center justify-center whitespace-nowrap rounded text-sm font-semibold transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand disabled:pointer-events-none disabled:opacity-50 disabled:cursor-not-allowed',
  {
    variants: {
      variant: {
        default: 'bg-brand text-on-brand hover:bg-brand-hover',
        destructive:
          'border border-error/40 bg-secondary text-error hover:bg-error-soft',
        outline: 'border border-border bg-secondary text-high hover:bg-panel',
        secondary: 'border border-border bg-secondary text-high hover:bg-panel',
        ghost: 'bg-transparent text-normal hover:bg-panel hover:text-high',
        link: 'text-brand-secondary underline-offset-4 hover:underline',
        icon: 'bg-transparent rounded text-low hover:text-high',
      },
      size: {
        default: 'h-8 px-3',
        xs: 'h-6 px-2 text-xs',
        sm: 'h-7 px-2.5',
        lg: 'h-9 px-3.5',
        icon: 'h-8 w-8',
      },
    },
    compoundVariants: [{ variant: 'icon', class: 'p-0 h-4' }],
    defaultVariants: {
      variant: 'default',
      size: 'default',
    },
  }
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : 'button';
    return (
      <Comp
        className={twMerge(cn(buttonVariants({ variant, size, className })))}
        ref={ref}
        {...props}
      />
    );
  }
);
Button.displayName = 'Button';

export { Button, buttonVariants };
