import { useCallback, type ComponentProps } from 'react';
import { useTranslation } from 'react-i18next';
import { Unplug, type LucideIcon } from 'lucide-react';
import { SettingsDialog } from '@/shared/dialogs/settings/SettingsDialog';

import { Alert } from '@prepcook/ui/components/Alert';
import { Button } from '@prepcook/ui/components/Button';
import { cn } from '@/shared/lib/utils';

interface BoardRequiredPromptProps {
  className?: string;
  buttonVariant?: ComponentProps<typeof Button>['variant'];
  buttonSize?: ComponentProps<typeof Button>['size'];
  buttonClassName?: string;
  title?: string;
  description?: string;
  actionLabel?: string;
  onAction?: () => void;
  icon?: LucideIcon;
}

/**
 * Shown in place of a view that needs the board while this server cannot
 * reach one. The action opens Settings → Board, which shows why.
 */
export function BoardRequiredPrompt({
  className,
  buttonVariant = 'outline',
  buttonSize = 'sm',
  buttonClassName,
  title,
  description,
  actionLabel,
  onAction,
  icon,
}: BoardRequiredPromptProps) {
  const { t } = useTranslation('tasks');

  const handleRedirect = useCallback(() => {
    if (onAction) {
      onAction();
      return;
    }
    void SettingsDialog.show({ initialSection: 'board' });
  }, [onAction]);

  const Icon = icon ?? Unplug;

  return (
    <Alert
      variant="default"
      className={cn('flex items-start gap-3', className)}
    >
      <Icon className="h-5 w-5 mt-0.5 text-muted-foreground" />
      <div className="space-y-2">
        <div className="font-medium">
          {title ?? t('shareDialog.boardRequired.title')}
        </div>
        <p className="text-sm text-muted-foreground">
          {description ?? t('shareDialog.boardRequired.description')}
        </p>
        <Button
          variant={buttonVariant}
          size={buttonSize}
          onClick={handleRedirect}
          className={cn('gap-2', buttonClassName)}
        >
          <Icon className="h-4 w-4" />
          {actionLabel ?? t('shareDialog.boardRequired.action')}
        </Button>
      </div>
    </Alert>
  );
}
