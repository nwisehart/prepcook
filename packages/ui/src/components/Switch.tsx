import * as React from 'react';
import * as SwitchPrimitives from '@radix-ui/react-switch';
import { cn } from '../lib/cn';

// 34x20 track (border color off, brand on) with a 16px white thumb inset 2px.
const switchRootClassName =
  'peer inline-flex h-5 w-[34px] shrink-0 cursor-pointer items-center ' +
  'rounded-full border-2 border-transparent transition-colors ' +
  'data-[state=checked]:bg-brand ' +
  'data-[state=unchecked]:bg-border ' +
  'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-secondary ' +
  'disabled:cursor-not-allowed disabled:opacity-50';

const switchThumbClassName =
  'pointer-events-none block size-4 rounded-full bg-on-brand shadow-sm ring-0 ' +
  'transition-transform data-[state=checked]:translate-x-3.5 ' +
  'data-[state=unchecked]:translate-x-0';

const Switch = React.forwardRef<
  React.ElementRef<typeof SwitchPrimitives.Root>,
  React.ComponentPropsWithoutRef<typeof SwitchPrimitives.Root>
>(({ className, ...props }, ref) => (
  <SwitchPrimitives.Root
    ref={ref}
    className={cn(switchRootClassName, className)}
    {...props}
  >
    <SwitchPrimitives.Thumb className={switchThumbClassName} />
  </SwitchPrimitives.Root>
));
Switch.displayName = SwitchPrimitives.Root.displayName;

export { Switch };
