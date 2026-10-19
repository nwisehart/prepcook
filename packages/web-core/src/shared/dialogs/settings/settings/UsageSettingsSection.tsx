import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { SpinnerIcon } from '@phosphor-icons/react';
import type { UsageSummary } from 'shared/types';
import { configApi } from '@/shared/lib/api';
import {
  ButtonGroup,
  ButtonGroupItem,
} from '@prepcook/ui/components/IconButtonGroup';
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
        <ButtonGroup>
          {WINDOWS.map((window) => (
            <ButtonGroupItem
              key={window}
              active={days === window}
              onClick={() => setDays(window)}
            >
              {t(`settings.usage.window${window}`)}
            </ButtonGroupItem>
          ))}
        </ButtonGroup>
      }
    >
      {isLoading && (
        <p className="flex items-center gap-2 text-sm text-low">
          <SpinnerIcon className="size-4 animate-spin" />
          {t('settings.usage.loading')}
        </p>
      )}
      {isError && (
        <p className="text-sm text-error">{t('settings.usage.error')}</p>
      )}
      {empty && <p className="text-sm text-low">{t('settings.usage.empty')}</p>}

      {totals && !empty && (
        <>
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-5">
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
                className="flex flex-col gap-0.5 rounded-lg border border-border bg-surface-raised px-3 py-2.5"
              >
                <div className="text-xl font-semibold tabular-nums text-high">
                  {fmt(value)}
                </div>
                <div className="text-xs text-low">
                  {t(`settings.usage.totals.${key}`)}
                </div>
              </div>
            ))}
          </div>

          {(data?.models?.length ?? 0) > 0 && (
            <div className="flex flex-col gap-1.5">
              <p className="text-[11px] font-bold uppercase tracking-[0.08em] text-low">
                {t('settings.usage.models.title')}
              </p>
              <div className="flex flex-col">
                {data?.models.map((model, index) => (
                  <div
                    key={model.model ?? 'unknown'}
                    className={
                      index > 0
                        ? 'flex items-center justify-between gap-3 border-t border-divider py-1.5 text-[13px]'
                        : 'flex items-center justify-between gap-3 py-1.5 text-[13px]'
                    }
                  >
                    <span className="truncate font-mono text-[12.5px] text-high">
                      {model.model ?? 'unknown'}
                    </span>
                    <span className="shrink-0 tabular-nums text-low">
                      {fmt(model.turns)} · {fmt(model.input_tokens)} in ·{' '}
                      {fmt(model.output_tokens)} out
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {topWorkspaces.length > 0 && (
            <div className="flex flex-col gap-1.5">
              <p className="text-[11px] font-bold uppercase tracking-[0.08em] text-low">
                {t('settings.usage.workspaces.title')}
              </p>
              <div className="flex flex-col">
                {topWorkspaces.map((workspace, index) => (
                  <div
                    key={workspace.workspace_id}
                    className={
                      index > 0
                        ? 'flex items-center justify-between gap-3 border-t border-divider py-1.5 text-[13px]'
                        : 'flex items-center justify-between gap-3 py-1.5 text-[13px]'
                    }
                  >
                    <span className="truncate text-normal">
                      {workspace.workspace_name ?? workspace.branch}
                    </span>
                    <span className="shrink-0 tabular-nums text-low">
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
