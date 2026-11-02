import { useEffect, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { PlusIcon, TrashIcon } from '@phosphor-icons/react';
import type { Descriptor, Field, TestOutcome } from 'shared/types';
import { integrationsApi } from '@/shared/lib/api';
import { cn } from '@/shared/lib/utils';
import { IconButton } from '@prepcook/ui/components/IconButton';
import { PrimaryButton } from '@prepcook/ui/components/PrimaryButton';
import {
  SettingsCheckbox,
  SettingsField,
  SettingsInput,
  SettingsSelect,
} from './SettingsComponents';

/** One configured provider: its `kind` plus the fields its descriptor names. */
export type IntegrationEntry = { kind: string } & Record<string, unknown>;

/** Every area's descriptors, fetched once per Settings session. */
export function useIntegrationDescriptors() {
  return useQuery({
    queryKey: ['integration-descriptors'],
    queryFn: () => integrationsApi.descriptors(),
    staleTime: Infinity,
  });
}

interface IntegrationSectionProps<E extends IntegrationEntry> {
  /** The area name, as the descriptors route keys it. */
  area: string;
  descriptors: Descriptor[];
  entries: E[];
  onChange: (entries: E[]) => void;
  /** Whether the area allows several entries at once. */
  multiple?: boolean;
  /** Area-specific controls rendered under an entry's own fields. */
  renderExtra?: (entry: E, update: (entry: E) => void) => ReactNode;
}

/** The first provider that can be chosen. */
const firstAvailable = (descriptors: Descriptor[]): string | undefined =>
  descriptors.find((descriptor) => descriptor.available)?.kind;

const fieldText = (value: unknown): string =>
  value == null ? '' : typeof value === 'string' ? value : String(value);

/** The entry's own fields, without the keys every entry carries. */
const testFields = (entry: IntegrationEntry): Record<string, unknown> => {
  const fields: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(entry)) {
    if (key !== 'kind' && key !== 'events') fields[key] = value;
  }
  return fields;
};

/** One descriptor field as a Settings input, by the field's kind. */
export function FieldInput({
  field,
  value,
  onChange,
  id,
}: {
  field: Field;
  value: unknown;
  onChange: (value: unknown) => void;
  id: string;
}) {
  const { t } = useTranslation('settings');
  const label = field.required
    ? `${field.label} (${t('settings.integrations.section.required')})`
    : field.label;
  switch (field.kind.type) {
    case 'bool':
      return (
        <SettingsCheckbox
          id={id}
          label={label}
          description={field.help}
          checked={value === true}
          onChange={(checked) => onChange(checked)}
        />
      );
    case 'select': {
      const options = field.kind.options.map((option) => ({
        value: option.value,
        label: option.label,
      }));
      return (
        <SettingsField label={label} description={field.help}>
          <SettingsSelect<string>
            value={fieldText(value) || undefined}
            options={options}
            onChange={(next) => onChange(next)}
          />
        </SettingsField>
      );
    }
    case 'number':
      return (
        <SettingsField label={label} description={field.help}>
          <SettingsInput
            value={fieldText(value)}
            onChange={(next) => {
              const parsed = Number(next);
              onChange(
                next.trim() === '' || Number.isNaN(parsed) ? null : parsed
              );
            }}
          />
        </SettingsField>
      );
    default:
      return (
        <SettingsField label={label} description={field.help}>
          <SettingsInput
            value={fieldText(value)}
            type={field.kind.type === 'secret' ? 'password' : 'text'}
            monospace={field.kind.type === 'secret'}
            autoComplete="off"
            onChange={(next) => onChange(next.trim() === '' ? null : next)}
          />
        </SettingsField>
      );
  }
}

/**
 * One integration area's configured providers, rendered entirely from the
 * providers' descriptors: a block per entry with its fields and a Test
 * connection button, and a picker to add another. Holds no state of its own
 * beyond test results; the caller owns the entries and saving.
 */
export function IntegrationSection<E extends IntegrationEntry>({
  area,
  descriptors,
  entries,
  onChange,
  multiple = false,
  renderExtra,
}: IntegrationSectionProps<E>) {
  const { t } = useTranslation('settings');
  const [newKind, setNewKind] = useState<string | undefined>(() =>
    firstAvailable(descriptors)
  );
  // The offered providers can change under the picker (another mode).
  useEffect(() => {
    setNewKind((current) =>
      descriptors.some((d) => d.kind === current && d.available)
        ? current
        : firstAvailable(descriptors)
    );
  }, [descriptors]);
  const newDescriptor = descriptors.find((d) => d.kind === newKind);
  const [tests, setTests] = useState<
    Record<number, TestOutcome | 'running' | undefined>
  >({});

  const update = (index: number, entry: E) => {
    onChange(entries.map((current, i) => (i === index ? entry : current)));
    setTests((current) => ({ ...current, [index]: undefined }));
  };

  const remove = (index: number) => {
    onChange(entries.filter((_, i) => i !== index));
    setTests({});
  };

  const add = () => {
    if (!newKind || !newDescriptor?.available) return;
    onChange([...entries, { kind: newKind } as E]);
  };

  const runTest = async (index: number, entry: E) => {
    setTests((current) => ({ ...current, [index]: 'running' }));
    try {
      const outcome = await integrationsApi.test(
        area,
        entry.kind,
        testFields(entry)
      );
      setTests((current) => ({ ...current, [index]: outcome }));
    } catch (err) {
      setTests((current) => ({
        ...current,
        [index]: {
          ok: false,
          message: err instanceof Error ? err.message : String(err),
        },
      }));
    }
  };

  const canAdd = multiple || entries.length === 0;

  return (
    <div className="flex flex-col gap-4">
      {entries.length === 0 && (
        <p className="text-sm text-low">
          {t('settings.integrations.section.empty')}
        </p>
      )}
      {entries.map((entry, index) => {
        const descriptor = descriptors.find((d) => d.kind === entry.kind);
        const test = tests[index];
        return (
          <div
            key={index}
            className="flex flex-col gap-4 rounded-lg border border-border px-4 py-3"
          >
            <div className="flex items-start justify-between gap-2">
              <div className="flex min-w-0 flex-col gap-1">
                <div className="text-sm font-semibold text-high">
                  {descriptor?.display_name ?? entry.kind}
                </div>
                {descriptor && (
                  <p className="text-[12.5px] leading-[1.45] text-low">
                    {descriptor.summary}
                  </p>
                )}
              </div>
              <IconButton
                icon={TrashIcon}
                onClick={() => remove(index)}
                aria-label={t('settings.integrations.section.remove')}
                title={t('settings.integrations.section.remove')}
                className="hover:bg-error-soft hover:text-error"
              />
            </div>
            {descriptor ? (
              descriptor.fields.map((field) => (
                <FieldInput
                  key={field.key}
                  id={`${area}-${index}-${field.key}`}
                  field={field}
                  value={entry[field.key]}
                  onChange={(value) =>
                    update(index, { ...entry, [field.key]: value })
                  }
                />
              ))
            ) : (
              <p className="text-[12.5px] text-error">
                {t('settings.integrations.section.unknownKind', {
                  kind: entry.kind,
                })}
              </p>
            )}
            {renderExtra?.(entry, (next) => update(index, next))}
            {descriptor && (
              <div className="flex items-center gap-3 border-t border-divider pt-3">
                <PrimaryButton
                  variant="secondary"
                  value={
                    test === 'running'
                      ? t('settings.integrations.section.testing')
                      : t('settings.integrations.section.test')
                  }
                  actionIcon={test === 'running' ? 'spinner' : undefined}
                  disabled={test === 'running'}
                  onClick={() => runTest(index, entry)}
                />
                {test && test !== 'running' && (
                  <span
                    className={cn(
                      'inline-flex items-center gap-2 text-[12.5px] font-semibold',
                      test.ok ? 'text-success' : 'text-error'
                    )}
                  >
                    <span
                      className={cn(
                        'size-2 shrink-0 rounded-full',
                        test.ok ? 'bg-success' : 'bg-error'
                      )}
                    />
                    {test.message}
                  </span>
                )}
              </div>
            )}
          </div>
        );
      })}
      {canAdd && descriptors.length > 0 && (
        <div className="flex items-end gap-2">
          <div className="min-w-0 flex-1">
            <SettingsField label={t('settings.integrations.section.addLabel')}>
              <SettingsSelect<string>
                value={newKind}
                options={descriptors.map((d) => ({
                  value: d.kind,
                  label: d.available
                    ? d.display_name
                    : t('settings.integrations.section.unavailable', {
                        name: d.display_name,
                      }),
                  disabled: !d.available,
                }))}
                onChange={setNewKind}
              />
            </SettingsField>
          </div>
          <PrimaryButton
            variant="secondary"
            value={t('settings.integrations.section.add')}
            actionIcon={PlusIcon}
            disabled={!newDescriptor?.available}
            onClick={add}
            className="h-9"
          />
        </div>
      )}
    </div>
  );
}
