import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { SpinnerIcon, TrashIcon } from '@phosphor-icons/react';
import type { SecretsResponse } from 'shared/types';
import { PrimaryButton } from '@prepcook/ui/components/PrimaryButton';
import { secretsApi } from '@/shared/lib/api';
import {
  repoScriptsKeys,
  secretsKeys,
  useReposScripts,
  useSecrets,
} from '@/shared/hooks/useRepoScripts';
import { ConfirmDialog } from '@/shared/dialogs/shared/ConfirmDialog';
import {
  SettingsCard,
  SettingsField,
  SettingsInput,
  SettingsLoading,
} from './SettingsComponents';
import { useSettingsMachineClient } from './SettingsHostContext';

/** The rule the secrets store applies to names: an env-var identifier. */
const SECRET_NAME_PATTERN = /^[A-Za-z_][A-Za-z0-9_]*$/;

/**
 * The secrets store the repository scripts read. Only names and the file's
 * path ever come back from the server; a value typed here is sent once and
 * cleared from the form.
 */
export function SecretsSettingsSection() {
  const { t } = useTranslation(['settings', 'common']);
  const queryClient = useQueryClient();
  const secretsQuery = useSecrets();

  const [name, setName] = useState('');
  const [value, setValue] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [savedName, setSavedName] = useState<string | null>(null);

  const applyResponse = (response: SecretsResponse) => {
    queryClient.setQueryData(secretsKeys.all, response);
    // `required_secrets_missing` in each repository's scripts response
    // depends on the store.
    void queryClient.invalidateQueries({ queryKey: repoScriptsKeys.all });
  };

  const setMutation = useMutation({
    mutationFn: (input: { name: string; value: string }) =>
      secretsApi.set(input.name, input.value),
    onSuccess: (response, input) => {
      applyResponse(response);
      setName('');
      setValue('');
      setError(null);
      setSavedName(input.name);
      setTimeout(() => setSavedName(null), 3000);
    },
    onError: (err) => {
      setError(
        err instanceof Error ? err.message : t('settings.secrets.add.error')
      );
    },
  });

  const removeMutation = useMutation({
    mutationFn: (secretName: string) => secretsApi.remove(secretName),
    onSuccess: (response) => {
      applyResponse(response);
      setError(null);
    },
    onError: (err) => {
      setError(
        err instanceof Error ? err.message : t('settings.secrets.list.error')
      );
    },
  });

  const trimmedName = name.trim();
  const nameInvalid =
    trimmedName.length > 0 && !SECRET_NAME_PATTERN.test(trimmedName);
  const canSave =
    trimmedName.length > 0 &&
    !nameInvalid &&
    value.length > 0 &&
    !setMutation.isPending;

  const handleSave = () => {
    if (!canSave) return;
    setSavedName(null);
    setMutation.mutate({ name: trimmedName, value });
  };

  const handleRemove = async (secretName: string) => {
    const result = await ConfirmDialog.show({
      title: t('settings.secrets.remove.dialogTitle', { name: secretName }),
      message: t('settings.secrets.remove.dialogDescription'),
      confirmText: t('settings.secrets.remove.confirm'),
      cancelText: t('common:buttons.cancel'),
      variant: 'destructive',
    });
    if (result !== 'confirmed') return;
    removeMutation.mutate(secretName);
  };

  if (secretsQuery.isLoading) {
    return <SettingsLoading label={t('settings.secrets.loading')} />;
  }

  if (secretsQuery.error || !secretsQuery.data) {
    return (
      <div className="py-8">
        <div className="rounded-lg border border-error/30 bg-error-soft px-4 py-3 text-sm text-error">
          {secretsQuery.error instanceof Error
            ? secretsQuery.error.message
            : t('settings.secrets.loadError')}
        </div>
      </div>
    );
  }

  const { names, file } = secretsQuery.data;

  return (
    <>
      {error && (
        <div className="rounded-lg border border-error/30 bg-error-soft px-4 py-3 text-sm text-error">
          {error}
        </div>
      )}

      {savedName && (
        <div className="rounded-lg border border-success/30 bg-success-soft px-4 py-3 text-sm font-medium text-success">
          {t('settings.secrets.add.success', { name: savedName })}
        </div>
      )}

      <SettingsCard
        title={t('settings.secrets.title')}
        description={t('settings.secrets.description')}
      >
        <SettingsField
          label={t('settings.secrets.file.label')}
          description={t('settings.secrets.file.helper')}
        >
          <div className="flex min-h-9 items-center break-all rounded border border-border bg-surface-raised px-3 py-1.5 font-mono text-[13px] text-normal">
            {file}
          </div>
        </SettingsField>

        <SettingsField label={t('settings.secrets.list.label')}>
          {names.length === 0 ? (
            <p className="text-sm text-low">
              {t('settings.secrets.list.empty')}
            </p>
          ) : (
            <div className="divide-y divide-divider rounded-lg border border-border">
              {names.map((secretName) => (
                <div
                  key={secretName}
                  className="flex items-center justify-between gap-3 px-3 py-2"
                >
                  <span className="break-all font-mono text-[13px] text-high">
                    {secretName}
                  </span>
                  <PrimaryButton
                    variant="secondary"
                    value={t('settings.secrets.list.remove')}
                    actionIcon={
                      removeMutation.isPending &&
                      removeMutation.variables === secretName
                        ? 'spinner'
                        : TrashIcon
                    }
                    onClick={() => void handleRemove(secretName)}
                    disabled={removeMutation.isPending}
                  />
                </div>
              ))}
            </div>
          )}
        </SettingsField>
      </SettingsCard>

      <SettingsCard
        title={t('settings.secrets.add.title')}
        description={t('settings.secrets.add.description')}
      >
        <SettingsField
          label={t('settings.secrets.add.name.label')}
          description={t('settings.secrets.add.name.helper')}
          error={nameInvalid ? t('settings.secrets.add.name.invalid') : null}
        >
          <SettingsInput
            value={name}
            onChange={setName}
            placeholder={t('settings.secrets.add.name.placeholder')}
            error={nameInvalid}
            autoComplete="off"
            monospace
          />
        </SettingsField>

        <SettingsField
          label={t('settings.secrets.add.value.label')}
          description={t('settings.secrets.add.value.helper')}
        >
          <SettingsInput
            type="password"
            value={value}
            onChange={setValue}
            placeholder={t('settings.secrets.add.value.placeholder')}
            autoComplete="new-password"
          />
        </SettingsField>

        <div className="flex">
          <PrimaryButton
            value={t('settings.secrets.add.button')}
            actionIcon={setMutation.isPending ? 'spinner' : undefined}
            onClick={handleSave}
            disabled={!canSave}
          />
        </div>
      </SettingsCard>

      <RequiredSecretsCard />
    </>
  );
}

/** Per repository, the `[secrets] requires` names with no value yet. */
function RequiredSecretsCard() {
  const { t } = useTranslation('settings');
  const machineClient = useSettingsMachineClient();
  const reposQuery = useQuery({
    queryKey: [
      'repos',
      ...(machineClient?.queryScopeKey ?? ['machine', 'unselected']),
    ],
    queryFn: () => {
      if (!machineClient) {
        throw new Error('Machine client is required');
      }
      return machineClient.listRepos();
    },
    enabled: machineClient != null,
  });
  const repos = reposQuery.data ?? [];
  const { byRepoId, isLoading } = useReposScripts(repos.map((r) => r.id));

  const reposWithMissing = repos
    .map((repo) => ({
      repo,
      missing: byRepoId.get(repo.id)?.required_secrets_missing ?? [],
    }))
    .filter((entry) => entry.missing.length > 0);

  return (
    <SettingsCard
      title={t('settings.secrets.required.title')}
      description={t('settings.secrets.required.description')}
    >
      {reposQuery.isLoading || isLoading ? (
        <div className="flex items-center gap-2 py-1">
          <SpinnerIcon className="size-3.5 animate-spin text-low" />
          <span className="text-sm text-low">
            {t('settings.secrets.required.loading')}
          </span>
        </div>
      ) : reposWithMissing.length === 0 ? (
        <p className="text-sm text-low">
          {t('settings.secrets.required.allSet')}
        </p>
      ) : (
        <div className="flex flex-col gap-3">
          {reposWithMissing.map(({ repo, missing }) => (
            <div key={repo.id} className="flex flex-col gap-1.5">
              <p className="text-sm font-semibold text-high">
                {repo.display_name}
              </p>
              <div className="flex flex-wrap gap-2">
                {missing.map((secretName) => (
                  <span
                    key={secretName}
                    className="inline-flex h-[22px] items-center gap-1.5 rounded-full bg-error-soft px-2 text-xs font-medium text-error"
                  >
                    <span className="size-2 rounded-full bg-error" />
                    <span className="font-mono">{secretName}</span>
                  </span>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </SettingsCard>
  );
}
