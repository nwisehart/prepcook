import { useTranslation } from 'react-i18next';
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
      <div className="flex flex-col gap-3">
        <div className="flex items-center gap-3 text-sm">
          <span className="w-12 shrink-0 font-semibold text-high">
            {t('settings.board.url')}
          </span>
          <code className="break-all font-mono text-[13px] text-normal">
            {boardUrl || t('settings.board.noUrl')}
          </code>
        </div>
        <div className="flex items-center gap-2">
          {connected ? (
            <span className="inline-flex items-center gap-2 text-[13px] font-semibold text-success">
              <span className="size-2 rounded-full bg-success" />
              {statusLabel}
            </span>
          ) : (
            <span className="inline-flex items-center gap-2 text-[13px] font-semibold text-attention-foreground">
              <span className="size-2 rounded-full bg-attention" />
              {statusLabel}
            </span>
          )}
        </div>
      </div>
    </SettingsCard>
  );
}
