import { useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  ArrowClockwiseIcon,
  CheckCircleIcon,
  CopyIcon,
  WarningCircleIcon,
} from '@phosphor-icons/react';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { configApi } from '@/shared/lib/api';
import { PrimaryButton } from '@prepcook/ui/components/PrimaryButton';
import { SettingsCard, SettingsField } from './SettingsComponents';

// The runner authenticates Claude Code with a subscription *seat* (an OAuth
// login, not an API key). Re-auth is inherently an interactive browser flow on
// the box (`claude /login` needs a TTY), so this panel can't perform it — it
// surfaces live seat health, a one-click re-check, and the exact command to run.
const RELOGIN_COMMAND = 'sudo docker exec -it prepcook claude /login';

export function ClaudeSettingsSection() {
  const { t } = useTranslation(['settings', 'common']);
  const { claudeSeatDegraded, reloadSystem } = useUserSystem();

  const [checking, setChecking] = useState(false);
  const [checkError, setCheckError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const handleRecheck = useCallback(async () => {
    setChecking(true);
    setCheckError(null);
    try {
      await configApi.recheckClaudeSeat();
      // Refresh /api/info so the degraded flag (and the app-wide banner) reflect
      // the fresh probe result immediately.
      await reloadSystem();
    } catch (err) {
      setCheckError(t('settings.claude.recheck.error'));
      console.error('Error re-checking Claude seat:', err);
    } finally {
      setChecking(false);
    }
  }, [reloadSystem, t]);

  const handleCopy = useCallback(() => {
    void navigator.clipboard.writeText(RELOGIN_COMMAND);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }, []);

  return (
    <>
      <SettingsCard
        title={t('settings.claude.status.title')}
        description={t('settings.claude.status.description')}
        headerAction={
          <PrimaryButton
            variant="tertiary"
            value={t('settings.claude.recheck.button')}
            onClick={handleRecheck}
            disabled={checking}
            actionIcon={checking ? 'spinner' : ArrowClockwiseIcon}
          />
        }
      >
        <div className="flex items-center gap-2">
          {claudeSeatDegraded ? (
            <span className="inline-flex items-center gap-1.5 rounded-sm bg-error/10 border border-error/50 px-base py-half text-sm font-medium text-error">
              <WarningCircleIcon className="size-icon-sm" weight="fill" />
              {t('settings.claude.status.degraded')}
            </span>
          ) : (
            <span className="inline-flex items-center gap-1.5 rounded-sm bg-success/10 border border-success/50 px-base py-half text-sm font-medium text-success">
              <CheckCircleIcon className="size-icon-sm" weight="fill" />
              {t('settings.claude.status.healthy')}
            </span>
          )}
        </div>

        {checkError && <p className="text-sm text-error">{checkError}</p>}

        <p className="text-sm text-low">
          {claudeSeatDegraded
            ? t('settings.claude.status.degradedHelper')
            : t('settings.claude.status.healthyHelper')}
        </p>
      </SettingsCard>

      <SettingsCard
        title={t('settings.claude.reauth.title')}
        description={t('settings.claude.reauth.description')}
      >
        <SettingsField
          label={t('settings.claude.reauth.commandLabel')}
          description={t('settings.claude.reauth.commandHelper')}
        >
          <div className="flex items-stretch gap-2">
            <code className="flex-1 min-w-0 overflow-x-auto whitespace-nowrap rounded-sm border border-border bg-secondary px-base py-half font-mono text-sm text-high">
              {RELOGIN_COMMAND}
            </code>
            <PrimaryButton
              variant="tertiary"
              value={
                copied
                  ? t('common:actions.copied')
                  : t('settings.claude.reauth.copy')
              }
              onClick={handleCopy}
              actionIcon={copied ? CheckCircleIcon : CopyIcon}
            />
          </div>
        </SettingsField>

        <ol className="list-decimal pl-5 space-y-1 text-sm text-low">
          <li>{t('settings.claude.reauth.step1')}</li>
          <li>{t('settings.claude.reauth.step2')}</li>
          <li>{t('settings.claude.reauth.step3')}</li>
        </ol>
      </SettingsCard>
    </>
  );
}
