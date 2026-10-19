import { Switch } from './Switch';
import { cn } from '../lib/cn';

interface ToggleProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  label: string;
  description?: string;
  disabled?: boolean;
  className?: string;
}

export function Toggle({
  checked,
  onCheckedChange,
  label,
  description,
  disabled,
  className,
}: ToggleProps) {
  return (
    <div className={cn('flex items-start gap-base', className)}>
      <Switch
        checked={checked}
        onCheckedChange={onCheckedChange}
        disabled={disabled}
        className="mt-0.5 shrink-0"
      />
      <div className="flex flex-col gap-half min-w-0">
        <span className="text-sm font-medium text-high">{label}</span>
        {description && <span className="text-xs text-low">{description}</span>}
      </div>
    </div>
  );
}
