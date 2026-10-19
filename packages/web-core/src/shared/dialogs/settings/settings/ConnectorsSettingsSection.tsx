import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import {
  ArrowClockwiseIcon,
  CheckCircleIcon,
  QuestionIcon,
  SpinnerIcon,
  WarningCircleIcon,
} from '@phosphor-icons/react';
import { configApi } from '@/shared/lib/api';
import { PrimaryButton } from '@prepcook/ui/components/PrimaryButton';
import { SettingsCard } from './SettingsComponents';

// Read-only visibility into the MCP servers every agent session loads
// (`claude mcp list` run server-side). Fixing auth is deliberately manual:
// remote-server OAuth runs via `/mcp` in an interactive `claude` session on
// the runner, and claude.ai account connectors are managed at claude.ai —
// neither flow can be embedded here.
export function ConnectorsSettingsSection() {
  const { t } = useTranslation(['settings', 'common']);
  const { data, isLoading, isFetching, refetch } = useQuery({
    queryKey: ['claude-connectors'],
    queryFn: () => configApi.getClaudeConnectors(),
    staleTime: 60 * 1000,
  });

  return (
    <SettingsCard
      title={t('settings.connectors.title')}
      description={t('settings.connectors.description')}
      headerAction={
        <PrimaryButton
          variant="secondary"
          value={t('settings.connectors.refresh')}
          onClick={() => void refetch()}
          disabled={isFetching}
          actionIcon={isFetching ? 'spinner' : ArrowClockwiseIcon}
        />
      }
    >
      {isLoading ? (
        <div className="flex items-center gap-2 py-2">
          <SpinnerIcon className="size-4 animate-spin text-brand" />
          <span className="text-sm text-normal">
            {t('settings.connectors.probing')}
          </span>
        </div>
      ) : !data?.available ? (
        <div className="rounded-lg border border-error/30 bg-error-soft px-4 py-3 text-sm text-error">
          {t('settings.connectors.unavailable')}
          {data?.raw && (
            <pre className="mt-2 whitespace-pre-wrap font-mono text-xs text-normal">
              {data.raw}
            </pre>
          )}
        </div>
      ) : (
        <>
          <div className="flex flex-col">
            {data.connectors.length === 0 && (
              <p className="text-sm text-low">
                {t('settings.connectors.empty')}
              </p>
            )}
            {data.connectors.map((connector, index) => (
              <div
                key={connector.name}
                className={
                  index > 0
                    ? 'flex items-start gap-2.5 border-t border-divider py-2'
                    : 'flex items-start gap-2.5 py-2'
                }
              >
                {connector.healthy === true ? (
                  <CheckCircleIcon
                    className="size-4 text-success mt-0.5 shrink-0"
                    weight="fill"
                  />
                ) : connector.healthy === false ? (
                  <WarningCircleIcon
                    className="size-4 text-error mt-0.5 shrink-0"
                    weight="fill"
                  />
                ) : (
                  <QuestionIcon className="size-4 text-low mt-0.5 shrink-0" />
                )}
                <div className="min-w-0">
                  <div className="text-sm font-semibold text-high">
                    {connector.name}
                  </div>
                  <div className="text-xs text-low break-all">
                    {connector.detail}
                  </div>
                </div>
              </div>
            ))}
          </div>
          <p className="text-[12.5px] leading-[1.45] text-low">
            {t('settings.connectors.fixHelper')}
          </p>
        </>
      )}
    </SettingsCard>
  );
}
