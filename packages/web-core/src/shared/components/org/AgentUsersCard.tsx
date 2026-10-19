import { useCallback, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import {
  ArrowsClockwiseIcon,
  CopyIcon,
  RobotIcon,
  SpinnerIcon,
} from '@phosphor-icons/react';
import { agentUsersApi, type AgentUserWithSecretT } from '@/shared/lib/api';
import { organizationKeys } from '@/shared/hooks/organizationKeys';
import { PrimaryButton } from '@prepcook/ui/components/PrimaryButton';
import {
  SettingsCard,
  SettingsField,
  SettingsInput,
} from '@/shared/dialogs/settings/settings/SettingsComponents';

/**
 * Admin-only management for agent users (non-SSO automation identities):
 * list, create, and rotate. Secrets are server-generated and displayed
 * exactly once — the runner stores them; the board never sees them again.
 */
export function AgentUsersCard({ organizationId }: { organizationId: string }) {
  const { t } = useTranslation('organization');
  const queryClient = useQueryClient();
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [revealed, setRevealed] = useState<AgentUserWithSecretT | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const { data: agents = [], isLoading } = useQuery({
    queryKey: ['agentUsers', organizationId],
    queryFn: () => agentUsersApi.list(organizationId),
    enabled: Boolean(organizationId),
  });

  const invalidate = useCallback(() => {
    void queryClient.invalidateQueries({
      queryKey: ['agentUsers', organizationId],
    });
    void queryClient.invalidateQueries({
      queryKey: organizationKeys.members(organizationId),
    });
  }, [queryClient, organizationId]);

  const createAgent = useMutation({
    mutationFn: () =>
      agentUsersApi.create(organizationId, {
        email: email.trim(),
        name: name.trim(),
      }),
    onSuccess: (result) => {
      setRevealed(result);
      setCopied(false);
      setError(null);
      setName('');
      setEmail('');
      invalidate();
    },
    onError: (err) =>
      setError(err instanceof Error ? err.message : t('agents.createError')),
  });

  const rotateAgent = useMutation({
    mutationFn: (userId: string) =>
      agentUsersApi.rotate(organizationId, userId),
    onSuccess: (result) => {
      setRevealed(result);
      setCopied(false);
      setError(null);
    },
    onError: (err) =>
      setError(err instanceof Error ? err.message : t('agents.rotateError')),
  });

  const handleCopySecret = useCallback(async () => {
    if (!revealed) return;
    try {
      await navigator.clipboard.writeText(revealed.secret);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Clipboard unavailable; the secret stays visible for manual copy.
    }
  }, [revealed]);

  return (
    <SettingsCard
      title={t('agents.title', 'Agent users')}
      description={t(
        'agents.description',
        'Automation identities for always-on runners. Agents sign in with email + secret (no SSO), get their own lane on the board, and wear a robot badge.'
      )}
    >
      {error && (
        <div className="bg-error/10 border border-error/50 rounded-sm p-3 text-sm text-error">
          {error}
        </div>
      )}

      {revealed && (
        <div className="bg-warning/10 border border-warning/50 rounded-sm p-3 space-y-1">
          <p className="text-sm font-medium text-high">
            {t('agents.secretTitle', {
              defaultValue: 'Secret for {{email}} — shown once',
              email: revealed.user.email,
            })}
          </p>
          <div className="flex items-center gap-2">
            <code className="flex-1 text-xs bg-panel rounded-sm px-2 py-1 break-all select-all">
              {revealed.secret}
            </code>
            <button
              type="button"
              onClick={() => void handleCopySecret()}
              className="shrink-0 p-1.5 rounded-sm hover:bg-secondary transition-colors"
              aria-label={t('agents.copySecret', 'Copy secret')}
            >
              <CopyIcon className="size-icon-xs" weight="bold" />
            </button>
            {copied && (
              <span className="text-xs text-success">
                {t('agents.copied', 'Copied')}
              </span>
            )}
          </div>
          <p className="text-xs text-low">
            {t(
              'agents.secretHelper',
              'Store it in the runner (sign-in email + this secret). It cannot be retrieved later — only rotated.'
            )}
          </p>
        </div>
      )}

      {isLoading ? (
        <div className="flex items-center justify-center py-4 gap-2">
          <SpinnerIcon className="size-icon-sm animate-spin" />
        </div>
      ) : agents.length === 0 ? (
        <p className="text-sm text-low">
          {t('agents.none', 'No agent users yet — create one below.')}
        </p>
      ) : (
        <div className="space-y-2">
          {agents.map((agent) => (
            <div
              key={agent.id}
              className="flex items-center justify-between p-3 border rounded-lg"
            >
              <div className="flex items-center gap-3 min-w-0">
                <RobotIcon
                  className="size-icon-base text-low shrink-0"
                  weight="bold"
                />
                <div className="min-w-0">
                  <div className="font-medium text-sm truncate">
                    {agent.first_name ?? agent.username ?? agent.email}
                  </div>
                  <div className="text-xs text-low truncate">{agent.email}</div>
                </div>
              </div>
              <button
                type="button"
                onClick={() => rotateAgent.mutate(agent.id)}
                disabled={rotateAgent.isPending}
                className="flex items-center gap-1.5 px-base py-half rounded-sm text-xs font-medium bg-secondary hover:bg-tertiary transition-colors disabled:opacity-50 shrink-0"
              >
                <ArrowsClockwiseIcon className="size-icon-xs" weight="bold" />
                {t('agents.rotate', 'Rotate secret')}
              </button>
            </div>
          ))}
        </div>
      )}

      <SettingsField
        label={t('agents.createLabel', 'Create agent')}
        description={t(
          'agents.createHelper',
          'The email does not need a real inbox — it is only the sign-in identity.'
        )}
      >
        <div className="flex flex-col sm:flex-row gap-2">
          <SettingsInput
            value={name}
            onChange={setName}
            placeholder={t('agents.namePlaceholder', 'Prepcook')}
          />
          <SettingsInput
            value={email}
            onChange={setEmail}
            placeholder="agent@example.com"
          />
          <PrimaryButton
            variant="secondary"
            value={
              createAgent.isPending
                ? t('agents.creating', 'Creating…')
                : t('agents.createButton', 'Create')
            }
            onClick={() => createAgent.mutate()}
            disabled={
              createAgent.isPending ||
              name.trim() === '' ||
              !email.includes('@')
            }
          />
        </div>
      </SettingsField>
    </SettingsCard>
  );
}
