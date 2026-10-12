import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { SpinnerIcon } from '@phosphor-icons/react';
import type { UsageSummary } from 'shared/types';
import { configApi } from '@/shared/lib/api';
import { PrimaryButton } from '@prepcook/ui/components/PrimaryButton';
import { SettingsCard } from './SettingsComponents';

// Token spend recorded per agent turn (execution_process_usage), rolled up
// by GET /api/usage. Numbers arrive as plain JSON numbers even though the
// generated types say bigint — always render through Number().
const WINDOWS = [7, 30] as const;

function fmt(value: unknown): string {
  return new Intl.NumberFormat(undefined, {
    notation: 'compact',
    maximumFractionDigits: 1,
  }).format(Number(value ?? 0));
}

export function UsageSettingsSection() {
  const { t } = useTranslation(['settings']);
  const [days, setDays] = useState<(typeof WINDOWS)[number]>(7);

  const { data, isLoading, isError } = useQuery<UsageSummary>({
    queryKey: ['usage', days],
    queryFn: () => configApi.getUsage(days),
    staleTime: 60_000,
  });

  const totals = data?.totals;
  const empty = totals != null && Number(totals.turns) === 0;
  const topWorkspaces = (data?.workspaces ?? [])
    .slice()
    .sort(
      (a, b) =>
        Number(b.input_tokens) +
        Number(b.output_tokens) -
        (Number(a.input_tokens) + Number(a.output_tokens))
    )
    .slice(0, 5);

  return (
    <SettingsCard
      title={t('settings.usage.title')}
      description={t('settings.usage.description')}
      headerAction={
        <div className="flex items-center gap-1">
          {WINDOWS.map((window) => (
            <PrimaryButton
              key={window}
              variant="tertiary"
              value={t(`settings.usage.window${window}`)}
              onClick={() => setDays(window)}
              disabled={days === window}
            />
          ))}
        </div>
      }
    >
      {isLoading && (
        <p className="flex items-center gap-1.5 text-sm text-low">
          <SpinnerIcon className="size-icon-sm animate-spin" />
          {t('settings.usage.loading')}
        </p>
      )}
      {isError && (
        <p className="text-sm text-error">{t('settings.usage.error')}</p>
      )}
      {empty && <p className="text-sm text-low">{t('settings.usage.empty')}</p>}

      {totals && !empty && (
        <>
          <div className="grid grid-cols-5 gap-2">
            {(
              [
                ['turns', totals.turns],
                ['input', totals.input_tokens],
                ['output', totals.output_tokens],
                ['cacheRead', totals.cache_read_tokens],
                ['cacheWrite', totals.cache_creation_tokens],
              ] as const
            ).map(([key, value]) => (
              <div
                key={key}
                className="rounded-sm border border-border bg-secondary px-base py-half"
              >
                <div className="text-lg font-semibold text-high">
                  {fmt(value)}
                </div>
                <div className="text-xs text-low">
                  {t(`settings.usage.totals.${key}`)}
                </div>
              </div>
            ))}
          </div>

          {(data?.models?.length ?? 0) > 0 && (
            <div>
              <p className="text-sm font-medium text-high mb-1">
                {t('settings.usage.models.title')}
              </p>
              <div className="space-y-0.5">
                {data?.models.map((model) => (
                  <div
                    key={model.model ?? 'unknown'}
                    className="flex items-center justify-between text-sm"
                  >
                    <span className="font-mono text-low truncate">
                      {model.model ?? 'unknown'}
                    </span>
                    <span className="text-low shrink-0 pl-2">
                      {fmt(model.turns)} · {fmt(model.input_tokens)} in ·{' '}
                      {fmt(model.output_tokens)} out
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {topWorkspaces.length > 0 && (
            <div>
              <p className="text-sm font-medium text-high mb-1">
                {t('settings.usage.workspaces.title')}
              </p>
              <div className="space-y-0.5">
                {topWorkspaces.map((workspace) => (
                  <div
                    key={workspace.workspace_id}
                    className="flex items-center justify-between text-sm"
                  >
                    <span className="text-low truncate">
                      {workspace.workspace_name ?? workspace.branch}
                    </span>
                    <span className="text-low shrink-0 pl-2">
                      {fmt(workspace.input_tokens)} in ·{' '}
                      {fmt(workspace.output_tokens)} out
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </>
      )}
    </SettingsCard>
  );
}
