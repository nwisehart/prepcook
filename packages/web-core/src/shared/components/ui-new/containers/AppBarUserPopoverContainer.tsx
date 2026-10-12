import { useState } from 'react';
import { AppBarUserPopover } from '@prepcook/ui/components/AppBarUserPopover';
import { SettingsDialog } from '@/shared/dialogs/settings/SettingsDialog';

export function AppBarUserPopoverContainer() {
  const [open, setOpen] = useState(false);

  const handleSettings = async () => {
    setOpen(false);
    await SettingsDialog.show();
  };

  return (
    <AppBarUserPopover
      open={open}
      onOpenChange={setOpen}
      onSettings={handleSettings}
    />
  );
}
