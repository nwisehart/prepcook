import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import {
  createMachineClient,
  type MachineClient,
  type MachineTarget,
} from '@/shared/lib/machineClient';

export type SettingsHostTarget = MachineTarget;

/** The machine whose settings the dialog edits: always the one serving the UI. */
export function useSettingsHost(): SettingsHostTarget {
  const { t } = useTranslation('settings');

  return useMemo(
    () => ({
      id: 'local',
      label: t('settings.hostPicker.thisMachine', 'This machine'),
      kind: 'local',
    }),
    [t]
  );
}

export function useSettingsMachineClient(): MachineClient | null {
  const selectedHost = useSettingsHost();

  return useMemo(() => createMachineClient(selectedHost), [selectedHost]);
}
