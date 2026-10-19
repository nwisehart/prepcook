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
            variant="secondary"
            value={t('settings.claude.recheck.button')}
            onClick={handleRecheck}
            disabled={checking}
            actionIcon={checking ? 'spinner' : ArrowClockwiseIcon}
          />
        }
      >
        <div className="flex items-center gap-2">
          {claudeSeatDegraded ? (
            <span className="inline-flex h-[22px] items-center gap-1.5 rounded-full bg-error-soft px-2 text-xs font-medium text-error">
              <WarningCircleIcon className="size-3.5" weight="fill" />
              {t('settings.claude.status.degraded')}
            </span>
          ) : (
            <span className="inline-flex h-[22px] items-center gap-1.5 rounded-full bg-success-soft px-2 text-xs font-medium text-success">
              <CheckCircleIcon className="size-3.5" weight="fill" />
              {t('settings.claude.status.healthy')}
            </span>
          )}
        </div>

        {checkError && <p className="text-[12.5px] text-error">{checkError}</p>}

        <p className="text-[12.5px] leading-[1.45] text-low">
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
            <code className="flex h-9 min-w-0 flex-1 items-center overflow-x-auto whitespace-nowrap rounded border border-border bg-surface-raised px-3 font-mono text-[13px] text-high">
              {RELOGIN_COMMAND}
            </code>
            <PrimaryButton
              variant="secondary"
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

        <ol className="flex list-decimal flex-col gap-1 pl-5 text-[13px] text-normal">
          <li>{t('settings.claude.reauth.step1')}</li>
          <li>{t('settings.claude.reauth.step2')}</li>
          <li>{t('settings.claude.reauth.step3')}</li>
        </ol>
      </SettingsCard>
    </>
  );
}
