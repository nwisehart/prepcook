import { useTranslation } from 'react-i18next';
import { CheckCircleIcon, WarningCircleIcon } from '@phosphor-icons/react';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { getRemoteApiUrl } from '@/shared/lib/remoteApi';
import { SettingsCard } from './SettingsComponents';

/**
 * The board this server works with: its URL and whether the server can
 * authenticate to it. The server signs its board requests with the board's
 * secret; there is nothing to sign in to.
 */
export function BoardSettingsSection() {
  const { t } = useTranslation('settings');
  const { boardStatus } = useUserSystem();
  const boardUrl = getRemoteApiUrl();

  const connected = boardStatus?.status === 'connected';
  const statusLabel = (() => {
    switch (boardStatus?.status) {
      case 'connected':
        return t('settings.board.connected');
      case 'not_answering':
        return t('settings.board.notAnswering');
      case 'no_board':
        return t('settings.board.noBoard');
      case 'no_secret':
      default:
        return t('settings.board.noSecret');
    }
  })();

  return (
    <SettingsCard
      title={t('settings.board.title')}
      description={t('settings.board.description')}
    >
      <div className="flex flex-col gap-base">
        <div className="flex items-center gap-2 text-sm">
          <span className="text-low">{t('settings.board.url')}</span>
          <code className="font-ibm-plex-mono text-high break-all">
            {boardUrl || t('settings.board.noUrl')}
          </code>
        </div>
        <div className="flex items-center gap-2">
          {connected ? (
            <span className="inline-flex items-center gap-1.5 rounded-sm bg-success/10 border border-success/50 px-base py-half text-sm font-medium text-success">
              <CheckCircleIcon className="size-icon-sm" weight="fill" />
              {statusLabel}
            </span>
          ) : (
            <span className="inline-flex items-center gap-1.5 rounded-sm bg-warning/10 border border-warning/50 px-base py-half text-sm font-medium text-warning">
              <WarningCircleIcon className="size-icon-sm" weight="fill" />
              {statusLabel}
            </span>
          )}
        </div>
      </div>
    </SettingsCard>
  );
}
