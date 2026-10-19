import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from './KeyboardDialog';
import { Button } from './Button';
import { Input } from './Input';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { FolderSimpleIcon, SpinnerIcon } from '@phosphor-icons/react';
import { defineModal } from '../lib/modals';

export interface CreateRepoDialogProps {
  onBrowseForPath?: (currentPath: string) => Promise<string | null | undefined>;
  onCreateRepo: (params: {
    parentPath: string;
    folderName: string;
  }) => Promise<void>;
}

export type CreateRepoDialogResult = {
  action: 'created' | 'canceled';
};

const CreateRepoDialogImpl = NiceModal.create<CreateRepoDialogProps>(
  ({ onBrowseForPath, onCreateRepo }) => {
    const { t } = useTranslation(['tasks', 'common']);
    const modal = useModal();

    const [name, setName] = useState('');
    const [parentPath, setParentPath] = useState('');
    const [isSubmitting, setIsSubmitting] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const handleBrowseForPath = useCallback(async () => {
      if (!onBrowseForPath) return;
      const selectedPath = await onBrowseForPath(parentPath);

      if (selectedPath) {
        setParentPath(selectedPath);
      }
    }, [onBrowseForPath, parentPath]);

    const handleCreate = useCallback(async () => {
      const trimmedName = name.trim();
      if (!trimmedName) {
        setError(t('git.createRepo.errors.nameRequired'));
        return;
      }

      setIsSubmitting(true);
      setError(null);
      try {
        await onCreateRepo({
          parentPath: parentPath.trim() || '.',
          folderName: trimmedName,
        });
        modal.resolve({ action: 'created' } as CreateRepoDialogResult);
        modal.hide();
      } catch (err) {
        setError(
          err instanceof Error
            ? err.message
            : t('git.createRepo.errors.createFailed')
        );
      } finally {
        setIsSubmitting(false);
      }
    }, [name, onCreateRepo, parentPath, modal, t]);

    const handleCancel = useCallback(() => {
      modal.resolve({ action: 'canceled' } as CreateRepoDialogResult);
      modal.hide();
    }, [modal]);

    const canSubmit = name.trim().length > 0;

    return (
      <Dialog open={modal.visible} onOpenChange={handleCancel}>
        <DialogContent className="sm:max-w-[425px]">
          <DialogHeader>
            <DialogTitle>{t('git.createRepo.dialog.title')}</DialogTitle>
            <DialogDescription>
              {t('git.createRepo.dialog.description')}
            </DialogDescription>
          </DialogHeader>

          <div className="flex flex-col gap-4 py-2">
            {/* Name input */}
            <div className="flex flex-col gap-1.5">
              <label
                htmlFor="create-repo-name"
                className="text-sm font-semibold text-high"
              >
                {t('git.createRepo.form.nameLabel')}
              </label>
              <Input
                id="create-repo-name"
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={t('git.createRepo.form.namePlaceholder')}
                disabled={isSubmitting}
              />
            </div>

            {/* Location input */}
            <div className="flex flex-col gap-1.5">
              <label
                htmlFor="create-repo-location"
                className="text-sm font-semibold text-high"
              >
                {t('git.createRepo.form.locationLabel')}
              </label>
              <div className="flex gap-2">
                <Input
                  id="create-repo-location"
                  type="text"
                  value={parentPath}
                  onChange={(e) => setParentPath(e.target.value)}
                  placeholder={t('git.createRepo.form.locationPlaceholder')}
                  disabled={isSubmitting}
                  className="font-mono text-[13px]"
                />
                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  className="size-9 shrink-0"
                  onClick={handleBrowseForPath}
                  disabled={isSubmitting || !onBrowseForPath}
                >
                  <FolderSimpleIcon className="size-4" />
                </Button>
              </div>
            </div>

            {/* Error */}
            {error && <p className="text-[12.5px] text-error">{error}</p>}
          </div>

          <DialogFooter>
            <Button
              variant="outline"
              onClick={handleCancel}
              disabled={isSubmitting}
            >
              {t('common:buttons.cancel')}
            </Button>
            <Button
              onClick={handleCreate}
              disabled={isSubmitting || !canSubmit}
            >
              {isSubmitting ? (
                <>
                  <SpinnerIcon className="size-4 animate-spin mr-2" />
                  {t('git.createRepo.states.creating')}
                </>
              ) : (
                t('git.createRepo.buttons.createRepository')
              )}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }
);

export const CreateRepoDialog = defineModal<
  CreateRepoDialogProps,
  CreateRepoDialogResult
>(CreateRepoDialogImpl);
