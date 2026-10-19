import { cn } from '../lib/cn';

interface RunningDotsProps {
  className?: string;
  /** Size override for each dot (defaults to the 6px `size-dot`) */
  dotClassName?: string;
}

/** Three brand dots pulsing in turn: the "working" indicator. */
export function RunningDots({ className, dotClassName }: RunningDotsProps) {
  const dot = cn('rounded-full bg-brand', dotClassName ?? 'size-dot');
  return (
    <div className={cn('flex items-center gap-[2px] shrink-0', className)}>
      <span className={cn(dot, 'animate-running-dot-1')} />
      <span className={cn(dot, 'animate-running-dot-2')} />
      <span className={cn(dot, 'animate-running-dot-3')} />
    </div>
  );
}
