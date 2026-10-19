import { WidgetProps } from '@rjsf/utils';
import { cn } from '@/shared/lib/utils';
import { useTranslation } from 'react-i18next';
import { useMemo } from 'react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  DropdownMenuTriggerButton,
} from '@prepcook/ui/components/Dropdown';
import { Switch } from '@prepcook/ui/components/Switch';

// TextWidget - Text input matching settings dialog styling
export const TextWidget = (props: WidgetProps) => {
  const {
    id,
    value,
    disabled,
    readonly,
    onChange,
    onBlur,
    onFocus,
    placeholder,
    options,
  } = props;

  const handleChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    const newValue = event.target.value;
    onChange(newValue === '' ? options.emptyValue : newValue);
  };

  const handleBlur = (event: React.FocusEvent<HTMLInputElement>) => {
    if (onBlur) {
      onBlur(id, event.target.value);
    }
  };

  const handleFocus = (event: React.FocusEvent<HTMLInputElement>) => {
    if (onFocus) {
      onFocus(id, event.target.value);
    }
  };

  return (
    <input
      id={id}
      type="text"
      value={value ?? ''}
      placeholder={placeholder || ''}
      disabled={disabled || readonly}
      onChange={handleChange}
      onBlur={handleBlur}
      onFocus={handleFocus}
      className={cn(
        'h-9 w-full rounded border border-border bg-secondary px-3 text-sm text-high',
        'placeholder:text-low focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
        (disabled || readonly) && 'opacity-50 cursor-not-allowed'
      )}
    />
  );
};

// SelectWidget - Dropdown select matching settings dialog styling
export const SelectWidget = (props: WidgetProps) => {
  const {
    id,
    value,
    disabled,
    readonly,
    onChange,
    options,
    schema,
    placeholder,
  } = props;

  const { t } = useTranslation('common');
  const { enumOptions } = options;

  const handleChange = (newValue: string) => {
    const finalValue = newValue === '__null__' ? options.emptyValue : newValue;
    onChange(finalValue);
  };

  // Handle nullable types
  const isNullable = Array.isArray(schema.type) && schema.type.includes('null');
  const allOptions = useMemo(() => {
    const selectOptions = enumOptions || [];
    if (isNullable) {
      return [
        { value: '__null__', label: t('form.notSpecified') },
        ...selectOptions.filter((opt) => opt.value !== null),
      ];
    }
    return selectOptions;
  }, [isNullable, enumOptions, t]);

  const currentValue = value === null ? '__null__' : (value ?? '');
  const selectedOption = allOptions.find(
    (opt) => String(opt.value) === String(currentValue)
  );

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <DropdownMenuTriggerButton
          id={id}
          label={selectedOption?.label || placeholder || t('form.selectOption')}
          className="h-9 w-full justify-between"
          disabled={disabled || readonly}
        />
      </DropdownMenuTrigger>
      <DropdownMenuContent className="w-[var(--radix-dropdown-menu-trigger-width)]">
        {allOptions.map((option) => (
          <DropdownMenuItem
            key={String(option.value)}
            onClick={() => handleChange(String(option.value))}
          >
            {option.label}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
};

// CheckboxWidget - A switch, like every boolean in Settings
// Note: Label is shown in the FieldTemplate's left column, not here
export const CheckboxWidget = (props: WidgetProps) => {
  const { id, value, disabled, readonly, onChange } = props;

  const handleChange = (checked: boolean) => {
    onChange(checked);
  };

  const checked = Boolean(value);

  return (
    <Switch
      id={id}
      checked={checked}
      onCheckedChange={handleChange}
      disabled={disabled || readonly}
    />
  );
};

// TextareaWidget - Textarea matching settings dialog styling
export const TextareaWidget = (props: WidgetProps) => {
  const {
    id,
    value,
    disabled,
    readonly,
    onChange,
    onBlur,
    onFocus,
    placeholder,
    options,
  } = props;

  const handleChange = (event: React.ChangeEvent<HTMLTextAreaElement>) => {
    const newValue = event.target.value;
    onChange(newValue === '' ? options.emptyValue : newValue);
  };

  const handleBlur = (event: React.FocusEvent<HTMLTextAreaElement>) => {
    if (onBlur) {
      onBlur(id, event.target.value);
    }
  };

  const handleFocus = (event: React.FocusEvent<HTMLTextAreaElement>) => {
    if (onFocus) {
      onFocus(id, event.target.value);
    }
  };

  return (
    <textarea
      id={id}
      value={value ?? ''}
      placeholder={placeholder || ''}
      disabled={disabled || readonly}
      onChange={handleChange}
      onBlur={handleBlur}
      onFocus={handleFocus}
      rows={4}
      className={cn(
        'w-full rounded border border-border bg-secondary px-3 py-2 text-sm text-high',
        'placeholder:text-low focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
        'resize-y',
        (disabled || readonly) && 'opacity-50 cursor-not-allowed'
      )}
    />
  );
};
