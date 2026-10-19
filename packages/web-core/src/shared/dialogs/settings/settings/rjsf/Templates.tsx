import type {
  FieldTemplateProps,
  ObjectFieldTemplateProps,
  ArrayFieldTemplateProps,
  ArrayFieldItemTemplateProps,
} from '@rjsf/utils';
import { PlusIcon, XIcon } from '@phosphor-icons/react';
import { cn } from '@/shared/lib/utils';
import { useTranslation } from 'react-i18next';
import { toPrettyCase } from '@/shared/lib/string';

// FieldTemplate - Two-column row: label and helper on the left, control on
// the right, matching the settings field type scale
export const FieldTemplate = (props: FieldTemplateProps) => {
  const {
    children,
    rawErrors = [],
    rawHelp,
    rawDescription,
    label,
    required,
    schema,
  } = props;

  if (schema.type === 'object') {
    return children;
  }

  return (
    <div className="grid grid-cols-1 gap-x-6 gap-y-2 py-3 sm:grid-cols-2">
      {/* Left column: Label and description */}
      <div className="flex min-w-0 flex-col gap-1">
        {label && (
          <div className="text-sm font-semibold text-high">
            {toPrettyCase(label)}
            {required && <span className="text-error ml-1">*</span>}
          </div>
        )}

        {rawDescription && (
          <p className="text-[12.5px] leading-[1.45] text-low">
            {rawDescription}
          </p>
        )}

        {rawHelp && (
          <p className="text-[12.5px] leading-[1.45] text-low">{rawHelp}</p>
        )}
      </div>

      {/* Right column: Field content */}
      <div className="flex min-w-0 flex-col gap-1.5">
        {children}

        {rawErrors.length > 0 && (
          <div className="flex flex-col gap-1">
            {rawErrors.map((error, index) => (
              <p key={index} className="text-[12.5px] text-error">
                {error}
              </p>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};

// ObjectFieldTemplate - Container for object fields
export const ObjectFieldTemplate = (props: ObjectFieldTemplateProps) => {
  const { properties } = props;

  return (
    <div className="divide-y divide-divider">
      {properties.map((element) => (
        <div key={element.name}>{element.content}</div>
      ))}
    </div>
  );
};

// ArrayFieldTemplate - Array field with add button
export const ArrayFieldTemplate = (props: ArrayFieldTemplateProps) => {
  const { t } = useTranslation('common');
  const { canAdd, items, onAddClick, disabled, readonly } = props;

  if (!items || (items.length === 0 && !canAdd)) {
    return null;
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-col gap-2">{items}</div>

      {canAdd && (
        <button
          type="button"
          onClick={onAddClick}
          disabled={disabled || readonly}
          className={cn(
            'flex h-8 w-full items-center justify-center gap-2 rounded px-3 text-sm font-semibold',
            'border border-border bg-secondary text-high hover:bg-panel',
            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
            'disabled:opacity-50 disabled:cursor-not-allowed transition-colors'
          )}
        >
          <PlusIcon className="size-4" />
          {t('buttons.addItem')}
        </button>
      )}
    </div>
  );
};

// ArrayFieldItemTemplate - Individual array item with remove button
export const ArrayFieldItemTemplate = (props: ArrayFieldItemTemplateProps) => {
  const { children, buttonsProps, disabled, readonly } = props;

  return (
    <div className="flex items-center gap-2">
      <div className="flex-1">{children}</div>

      {buttonsProps.hasRemove && (
        <button
          type="button"
          onClick={buttonsProps.onRemoveItem}
          disabled={disabled || readonly || buttonsProps.disabled}
          className={cn(
            'size-9 p-0 flex items-center justify-center shrink-0 rounded',
            'text-low hover:text-error hover:bg-error-soft',
            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
            'disabled:opacity-50 disabled:cursor-not-allowed transition-colors'
          )}
          title="Remove item"
        >
          <XIcon className="size-4" />
        </button>
      )}
    </div>
  );
};

// FormTemplate - Root form container
export const FormTemplate = ({ children }: React.PropsWithChildren) => {
  return <div className="w-full">{children}</div>;
};
