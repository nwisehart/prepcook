import type { ReactNode } from 'react';
import { cn } from '@/shared/lib/utils';
import { useTranslation } from 'react-i18next';
import { SpinnerIcon } from '@phosphor-icons/react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  DropdownMenuTriggerButton,
} from '@prepcook/ui/components/Dropdown';
import { Button } from '@prepcook/ui/components/Button';
import { Checkbox } from '@prepcook/ui/components/Checkbox';
import { Switch } from '@prepcook/ui/components/Switch';
import type { Icon } from '@phosphor-icons/react';

// Shared class strings, so raw elements inside a section match the inputs
// these components render.
export const settingsInputClassName =
  'h-9 w-full rounded border bg-secondary px-3 text-sm text-high ' +
  'placeholder:text-low focus-visible:outline-none focus-visible:ring-1 ' +
  'focus-visible:ring-inset focus-visible:ring-brand';

export const settingsHelperClassName = 'text-[12.5px] leading-[1.45] text-low';

/** A bordered row inside a settings card (an entry, a step, a provider). */
export const settingsInsetClassName =
  'rounded-lg border border-border bg-secondary px-4 py-3';

// ============================================================================
// Two-Column Picker Components
// ============================================================================

// TwoColumnPicker - Container for the two-column layout
interface TwoColumnPickerProps {
  children: ReactNode;
}

export function TwoColumnPicker({ children }: TwoColumnPickerProps) {
  return (
    <div className="flex flex-col md:flex-row border border-border rounded-lg overflow-hidden bg-secondary">
      {children}
    </div>
  );
}

// TwoColumnPickerColumn - A single column within the picker
interface TwoColumnPickerColumnProps {
  label: string;
  headerAction?: ReactNode;
  isFirst?: boolean;
  children: ReactNode;
}

export function TwoColumnPickerColumn({
  label,
  headerAction,
  isFirst,
  children,
}: TwoColumnPickerColumnProps) {
  return (
    <div
      className={cn(
        'flex-1 min-w-0',
        isFirst && 'border-b md:border-b-0 md:border-r border-border'
      )}
    >
      <div className="h-9 px-3 border-b border-border bg-surface-raised flex items-center justify-between">
        <span className="text-[11px] font-bold uppercase tracking-[0.08em] text-low">
          {label}
        </span>
        {headerAction}
      </div>
      <div className="max-h-40 md:h-40 overflow-y-auto p-1">{children}</div>
    </div>
  );
}

// TwoColumnPickerItem - A selectable row within a column
interface TwoColumnPickerItemProps {
  selected?: boolean;
  onClick?: () => void;
  leading?: ReactNode;
  trailing?: ReactNode;
  children: ReactNode;
}

export function TwoColumnPickerItem({
  selected,
  onClick,
  leading,
  trailing,
  children,
}: TwoColumnPickerItemProps) {
  return (
    <div
      className={cn(
        'group flex items-center gap-2 rounded px-2.5 py-1.5 cursor-pointer transition-colors',
        selected ? 'bg-panel' : 'hover:bg-panel'
      )}
      onClick={onClick}
    >
      {leading}
      <span
        className={cn(
          'text-sm truncate flex-1',
          selected ? 'text-high font-semibold' : 'text-normal font-medium'
        )}
      >
        {children}
      </span>
      {trailing}
    </div>
  );
}

// TwoColumnPickerBadge - A small badge/tag for items
interface TwoColumnPickerBadgeProps {
  variant?: 'default' | 'brand';
  children: ReactNode;
}

export function TwoColumnPickerBadge({
  variant = 'default',
  children,
}: TwoColumnPickerBadgeProps) {
  return (
    <span
      className={cn(
        'inline-flex h-5 items-center rounded-full px-2 text-xs font-medium shrink-0',
        variant === 'brand'
          ? 'bg-brand-soft text-brand-soft-foreground'
          : 'bg-panel text-normal'
      )}
    >
      {children}
    </span>
  );
}

// TwoColumnPickerEmpty - Empty state message for a column
interface TwoColumnPickerEmptyProps {
  children: ReactNode;
}

export function TwoColumnPickerEmpty({ children }: TwoColumnPickerEmptyProps) {
  return (
    <div className="px-3 py-plusfifty text-sm text-low text-center">
      {children}
    </div>
  );
}

// ============================================================================
// Settings Card Components
// ============================================================================

// SettingsCard - One logical group of fields: a white card on the page ground
export function SettingsCard({
  title,
  description,
  children,
  headerAction,
  className,
}: {
  title?: string;
  description?: ReactNode;
  children: React.ReactNode;
  headerAction?: React.ReactNode;
  className?: string;
}) {
  const hasHeader = Boolean(title || description || headerAction);
  return (
    <section
      className={cn(
        'flex flex-col gap-4 rounded-lg border border-border bg-secondary px-5 py-[18px]',
        className
      )}
    >
      {hasHeader && (
        <div className="flex items-start justify-between gap-3">
          <div className="flex min-w-0 flex-col gap-1">
            {title && (
              <h3 className="text-[15px] font-bold leading-snug text-high">
                {title}
              </h3>
            )}
            {description && (
              <p className={settingsHelperClassName}>{description}</p>
            )}
          </div>
          {headerAction && <div className="shrink-0">{headerAction}</div>}
        </div>
      )}
      {children}
    </section>
  );
}

// SettingsField - A labeled field: label, helper under it, then the control
export function SettingsField({
  label,
  description,
  error,
  children,
  className,
}: {
  label: string;
  description?: React.ReactNode;
  error?: string | null;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn('flex min-w-0 flex-col gap-1.5', className)}>
      {label && (
        <label className="text-sm font-semibold text-high">{label}</label>
      )}
      {description && <p className={settingsHelperClassName}>{description}</p>}
      {children}
      {error && <p className="text-[12.5px] text-error">{error}</p>}
    </div>
  );
}

// SettingsCheckbox - A boolean setting: a switch (or, for picking members of
// a list, a checkbox) with its label and optional helper
export function SettingsCheckbox({
  id,
  label,
  description,
  checked,
  onChange,
  disabled,
  variant = 'switch',
}: {
  id: string;
  label: string;
  description?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  variant?: 'switch' | 'checkbox';
}) {
  return (
    <div className="flex items-start gap-3">
      {variant === 'switch' ? (
        <Switch
          id={id}
          checked={checked}
          onCheckedChange={onChange}
          disabled={disabled}
          className="mt-px"
        />
      ) : (
        <Checkbox
          id={id}
          checked={checked}
          onCheckedChange={onChange}
          disabled={disabled}
          className="mt-[3px]"
        />
      )}
      <div className="flex min-w-0 flex-col gap-0.5">
        <label
          htmlFor={id}
          className={cn(
            'text-sm text-high cursor-pointer',
            description ? 'font-medium' : 'font-normal',
            disabled && 'opacity-50 cursor-not-allowed'
          )}
        >
          {label}
        </label>
        {description && (
          <p className={settingsHelperClassName}>{description}</p>
        )}
      </div>
    </div>
  );
}

// SettingsSelect - A dropdown select component
export function SettingsSelect<T extends string>({
  value,
  options,
  actions,
  onChange,
  placeholder,
  disabled,
  className,
}: {
  value: T | undefined;
  /** A disabled option is listed but cannot be chosen. */
  options: { value: T; label: string; disabled?: boolean }[];
  actions?: { label: string; icon?: Icon; onClick: () => void }[];
  onChange: (value: T) => void;
  placeholder?: string;
  disabled?: boolean;
  className?: string;
}) {
  const selectedOption = options.find((opt) => opt.value === value);
  const hasMenuItems = options.length > 0 || (actions?.length ?? 0) > 0;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <DropdownMenuTriggerButton
          label={selectedOption?.label || placeholder}
          className={cn('h-9 w-full justify-between', className)}
          disabled={disabled || !hasMenuItems}
        />
      </DropdownMenuTrigger>
      <DropdownMenuContent className="w-[var(--radix-dropdown-menu-trigger-width)]">
        {options.map((option) => (
          <DropdownMenuItem
            key={option.value}
            disabled={option.disabled}
            onClick={() => onChange(option.value)}
          >
            {option.label}
          </DropdownMenuItem>
        ))}
        {actions?.map((action) => (
          <DropdownMenuItem
            key={action.label}
            icon={action.icon}
            onClick={action.onClick}
          >
            {action.label}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// SettingsInput - A 36px text input field
export function SettingsInput({
  value,
  onChange,
  placeholder,
  error,
  disabled,
  type = 'text',
  autoComplete,
  monospace = false,
  className,
  id,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  error?: boolean;
  disabled?: boolean;
  type?: 'text' | 'password';
  autoComplete?: string;
  monospace?: boolean;
  className?: string;
  id?: string;
}) {
  return (
    <input
      id={id}
      type={type}
      autoComplete={autoComplete}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      disabled={disabled}
      className={cn(
        settingsInputClassName,
        monospace && 'font-mono text-[13px]',
        error ? 'border-error' : 'border-border',
        disabled && 'opacity-50 cursor-not-allowed',
        className
      )}
    />
  );
}

// SettingsTextarea - A multi-line text input
export function SettingsTextarea({
  value,
  onChange,
  placeholder,
  disabled,
  rows = 4,
  monospace = false,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
  rows?: number;
  monospace?: boolean;
}) {
  return (
    <textarea
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      disabled={disabled}
      rows={rows}
      className={cn(
        'w-full rounded border border-border bg-secondary px-3 py-2 text-sm text-high',
        'placeholder:text-low focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
        'resize-y',
        monospace && 'font-mono text-[13px] leading-relaxed',
        disabled && 'opacity-50 cursor-not-allowed'
      )}
    />
  );
}

// SettingsAlert - The saved / failed line at the top of a section
export function SettingsAlert({
  variant,
  children,
  className,
}: {
  variant: 'error' | 'success';
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      role={variant === 'error' ? 'alert' : 'status'}
      className={cn(
        'rounded-lg border px-4 py-3 text-sm',
        variant === 'error'
          ? 'border-error/30 bg-error-soft text-error'
          : 'border-success/30 bg-success-soft font-medium text-success',
        className
      )}
    >
      {children}
    </div>
  );
}

// SettingsLoading - The spinner a section shows while its data loads
export function SettingsLoading({ label }: { label: string }) {
  return (
    <div className="flex items-center justify-center gap-2 py-8">
      <SpinnerIcon className="size-icon-lg animate-spin text-brand" />
      <span className="text-sm text-normal">{label}</span>
    </div>
  );
}

// SettingsSaveBar - The section's bottom action row. Always shown so the
// save state is visible; Save is enabled only while there is something to
// save. Sticks to the bottom of the scrolling content.
export function SettingsSaveBar({
  show,
  saving,
  saveDisabled,
  onSave,
  onDiscard,
  onReset,
  resetDisabled,
}: {
  /** Whether the section has unsaved changes. */
  show: boolean;
  saving: boolean;
  saveDisabled?: boolean;
  unsavedMessage?: string;
  onSave: () => void;
  onDiscard?: () => void;
  /** Puts the section's defaults in the draft; nothing is saved until Save. */
  onReset?: () => void;
  resetDisabled?: boolean;
}) {
  const { t } = useTranslation(['settings', 'common']);

  return (
    <div className="sticky bottom-0 z-10 mt-auto flex flex-wrap items-center gap-3 border-t border-border bg-primary py-3">
      <span
        className={cn(
          'text-[12.5px]',
          show ? 'font-semibold text-high' : 'text-low'
        )}
      >
        {show
          ? t('settings.common.unsavedLabel')
          : t('settings.common.noUnsavedChanges')}
      </span>
      <span className="flex-1" />
      {onReset && (
        <Button
          variant="outline"
          size="lg"
          onClick={onReset}
          disabled={saving || resetDisabled}
        >
          {t('settings.common.resetToDefaults')}
        </Button>
      )}
      {onDiscard && show && (
        <Button variant="ghost" size="lg" onClick={onDiscard} disabled={saving}>
          {t('common:buttons.discard')}
        </Button>
      )}
      <Button
        size="lg"
        className="gap-2"
        onClick={onSave}
        disabled={!show || saving || saveDisabled}
      >
        {saving && <SpinnerIcon className="size-4 animate-spin" />}
        {t('common:buttons.save')}
      </Button>
    </div>
  );
}
