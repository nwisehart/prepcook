import { cn } from '../lib/cn';

export interface StatusDotProps {
  color: string;
  className?: string;
}

export const StatusDot = ({ color, className }: StatusDotProps) => (
  <span
    className={cn('size-2 rounded-full shrink-0', className)}
    style={{ backgroundColor: `hsl(${color})` }}
  />
);
