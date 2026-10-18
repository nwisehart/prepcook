import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { create, useModal } from '@ebay/nice-modal-react';
import { Loader2 } from 'lucide-react';
import { ArrowSquareOutIcon } from '@phosphor-icons/react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@prepcook/ui/components/KeyboardDialog';
import { Button } from '@prepcook/ui/components/Button';
import { Checkbox } from '@prepcook/ui/components/Checkbox';
import { Alert, AlertDescription } from '@prepcook/ui/components/Alert';
import { Badge } from '@prepcook/ui/components/Badge';
import type {
  TrackerImportCandidate,
  TrackerImportError,
  WorkClass,
} from 'shared/types';
import { trackerApi } from '@/shared/lib/api';
import { refreshFallbackSources } from '@/shared/lib/electric/collections';
import { defineModal } from '@/shared/lib/modals';
import { SettingsDialog } from '@/shared/dialogs/settings/SettingsDialog';
import { cn } from '@/shared/lib/utils';
import { WORK_CLASS_STYLES } from '@/shared/lib/workClass';

interface ImportFromTrackerDialogProps {
  projectId: string;
}

export type ImportFromTrackerDialogResult = {
  imported: number;
  skipped: number;
} | null;

type PreviewFailure = {
  error?: TrackerImportError;
  message?: string;
};

function WorkClassChip({ workClass }: { workClass: WorkClass }) {
  const { t } = useTranslation('common');
  return (
    <Badge
      variant="outline"
      className={cn(
        'border-transparent shrink-0 capitalize',
        WORK_CLASS_STYLES[workClass]
      )}
    >
      {t(`kanban.trackerImport.workClass.${workClass}`)}
    </Badge>
  );
}

const ImportFromTrackerDialogImpl = create<ImportFromTrackerDialogProps>(
  ({ projectId }) => {
    const modal = useModal();
    const { t } = useTranslation('common');

    const [loadingPreview, setLoadingPreview] = useState(false);
    const [candidates, setCandidates] = useState<
      TrackerImportCandidate[] | null
    >(null);
    const [selectedKeys, setSelectedKeys] = useState<Set<string>>(new Set());
    const [previewFailure, setPreviewFailure] = useState<PreviewFailure | null>(
      null
    );
    const [importing, setImporting] = useState(false);
    const [importError, setImportError] = useState<string | null>(null);
    const [successMessage, setSuccessMessage] = useState<string | null>(null);
    const closeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

    const closeDialog = useCallback(
      (result: ImportFromTrackerDialogResult) => {
        modal.resolve(result);
        modal.hide();
      },
      [modal]
    );

    // Load preview when the dialog opens
    useEffect(() => {
      if (!modal.visible) return;

      let isCancelled = false;

      const loadPreview = async () => {
        setLoadingPreview(true);
        setCandidates(null);
        setSelectedKeys(new Set());
        setPreviewFailure(null);
        setImportError(null);
        setSuccessMessage(null);

        const result = await trackerApi.preview({ project_id: projectId });
        if (isCancelled) return;

        setLoadingPreview(false);

        if (result.success) {
          setCandidates(result.data.candidates);
          setSelectedKeys(
            new Set(
              result.data.candidates
                .filter((candidate) => !candidate.already_imported)
                .map((candidate) => candidate.key)
            )
          );
        } else {
          setPreviewFailure({ error: result.error, message: result.message });
        }
      };

      void loadPreview();

      return () => {
        isCancelled = true;
      };
    }, [modal.visible, projectId]);

    // Clear pending auto-close timer on unmount
    useEffect(() => {
      return () => {
        if (closeTimerRef.current) {
          clearTimeout(closeTimerRef.current);
        }
      };
    }, []);

    const selectableCandidates = useMemo(
      () => (candidates ?? []).filter((c) => !c.already_imported),
      [candidates]
    );
    const allSelected =
      selectableCandidates.length > 0 &&
      selectableCandidates.every((c) => selectedKeys.has(c.key));

    const toggleCandidate = useCallback((key: string) => {
      setSelectedKeys((prev) => {
        const next = new Set(prev);
        if (next.has(key)) {
          next.delete(key);
        } else {
          next.add(key);
        }
        return next;
      });
    }, []);

    const toggleSelectAll = useCallback(() => {
      setSelectedKeys(
        allSelected
          ? new Set()
          : new Set(selectableCandidates.map((c) => c.key))
      );
    }, [allSelected, selectableCandidates]);

    const handleOpenTrackerSettings = useCallback(() => {
      closeDialog(null);
      void SettingsDialog.show({ initialSection: 'ticketing' });
    }, [closeDialog]);

    const handleImport = useCallback(async () => {
      if (selectedKeys.size === 0 || importing) return;

      setImporting(true);
      setImportError(null);

      const result = await trackerApi.import({
        project_id: projectId,
        keys: [...selectedKeys],
      });

      setImporting(false);

      if (result.success) {
        const { imported, skipped } = result.data;
        // The importer writes issues server-side; sync sources that fell
        // back to interval polling (30s) won't show them promptly unless
        // refreshed explicitly.
        if (imported > 0) {
          refreshFallbackSources();
        }
        setSuccessMessage(
          t('kanban.trackerImport.success', { imported, skipped })
        );
        closeTimerRef.current = setTimeout(() => {
          closeDialog({ imported, skipped });
        }, 1500);
        return;
      }

      if (result.error?.type === 'request_failed') {
        setImportError(result.error.message);
      } else if (result.error?.type === 'auth_failed') {
        setImportError(t('kanban.trackerImport.errors.authFailed'));
      } else if (result.error?.type === 'not_configured') {
        setImportError(t('kanban.trackerImport.errors.notConfigured'));
      } else if (result.error?.type === 'not_supported') {
        setImportError(
          t('kanban.trackerImport.errors.notSupported', {
            tracker: result.error.tracker,
          })
        );
      } else if (result.error?.type === 'remote_unavailable') {
        setImportError(t('kanban.trackerImport.errors.remoteUnavailable'));
      } else {
        setImportError(
          result.message ?? t('kanban.trackerImport.errors.importFailed')
        );
      }
    }, [closeDialog, importing, projectId, selectedKeys, t]);

    const renderPreviewFailure = (failure: PreviewFailure) => {
      switch (failure.error?.type) {
        case 'not_configured':
          return (
            <Alert variant="default">
              <AlertDescription className="space-y-3">
                <p>{t('kanban.trackerImport.errors.notConfigured')}</p>
                <Button variant="outline" onClick={handleOpenTrackerSettings}>
                  {t('kanban.trackerImport.errors.openSettings')}
                </Button>
              </AlertDescription>
            </Alert>
          );
        case 'not_supported':
          return (
            <Alert variant="default">
              <AlertDescription className="space-y-3">
                <p>
                  {t('kanban.trackerImport.errors.notSupported', {
                    tracker: failure.error.tracker,
                  })}
                </p>
                <Button variant="outline" onClick={handleOpenTrackerSettings}>
                  {t('kanban.trackerImport.errors.openSettings')}
                </Button>
              </AlertDescription>
            </Alert>
          );
        case 'auth_failed':
          return (
            <Alert variant="destructive">
              {t('kanban.trackerImport.errors.authFailed')}
            </Alert>
          );
        case 'request_failed':
          return <Alert variant="destructive">{failure.error.message}</Alert>;
        case 'remote_unavailable':
          return (
            <Alert variant="destructive">
              {t('kanban.trackerImport.errors.remoteUnavailable')}
            </Alert>
          );
        default:
          return (
            <Alert variant="destructive">
              {failure.message ??
                t('kanban.trackerImport.errors.previewFailed')}
            </Alert>
          );
      }
    };

    const renderCandidateList = (list: TrackerImportCandidate[]) => {
      if (list.length === 0) {
        return (
          <p className="py-8 text-center text-sm text-low">
            {t('kanban.trackerImport.empty')}
          </p>
        );
      }

      return (
        <div className="space-y-2">
          <div className="flex items-center gap-2 px-1">
            <Checkbox
              checked={allSelected}
              onCheckedChange={toggleSelectAll}
              disabled={selectableCandidates.length === 0}
              className="h-5 w-5"
            />
            <button
              type="button"
              className="text-sm text-normal hover:text-high"
              onClick={toggleSelectAll}
              disabled={selectableCandidates.length === 0}
            >
              {t('kanban.trackerImport.selectAll')}
            </button>
            <span className="ml-auto text-sm text-low">
              {t('kanban.trackerImport.selectedCount', {
                selected: selectedKeys.size,
                total: selectableCandidates.length,
              })}
            </span>
          </div>
          <div className="max-h-[320px] overflow-y-auto rounded-sm border border-border divide-y divide-border">
            {list.map((candidate) => (
              <div
                key={candidate.key}
                className={cn(
                  'flex items-center gap-3 px-3 py-2',
                  candidate.already_imported && 'opacity-60'
                )}
              >
                <Checkbox
                  checked={
                    !candidate.already_imported &&
                    selectedKeys.has(candidate.key)
                  }
                  onCheckedChange={() => toggleCandidate(candidate.key)}
                  disabled={candidate.already_imported}
                  className="h-5 w-5 shrink-0"
                />
                <a
                  href={candidate.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex items-center gap-1 shrink-0 font-mono text-sm text-brand hover:underline"
                  onClick={(e) => e.stopPropagation()}
                >
                  {candidate.key}
                  <ArrowSquareOutIcon className="h-3 w-3" weight="bold" />
                </a>
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm text-high">
                    {candidate.title}
                  </p>
                  <p className="truncate text-xs text-low">
                    {candidate.issue_type}
                  </p>
                </div>
                <WorkClassChip workClass={candidate.work_class} />
                {candidate.already_imported && (
                  <Badge variant="secondary" className="shrink-0">
                    {t('kanban.trackerImport.importedBadge')}
                  </Badge>
                )}
              </div>
            ))}
          </div>
        </div>
      );
    };

    return (
      <Dialog
        open={modal.visible}
        onOpenChange={(open) => {
          if (!open) {
            closeDialog(null);
          }
        }}
      >
        <DialogContent className="sm:max-w-[640px]">
          <DialogHeader>
            <DialogTitle>{t('kanban.trackerImport.title')}</DialogTitle>
            <DialogDescription>
              {t('kanban.trackerImport.description')}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-2">
            {loadingPreview ? (
              <div className="flex items-center justify-center gap-2 py-8">
                <Loader2 className="h-5 w-5 animate-spin text-low" />
                <span className="text-sm text-low">
                  {t('kanban.trackerImport.loading')}
                </span>
              </div>
            ) : previewFailure ? (
              renderPreviewFailure(previewFailure)
            ) : candidates ? (
              renderCandidateList(candidates)
            ) : null}

            {importError && <Alert variant="destructive">{importError}</Alert>}
            {successMessage && (
              <div className="bg-success/10 border border-success/50 rounded-sm p-3 text-success text-sm font-medium">
                {successMessage}
              </div>
            )}
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => closeDialog(null)}>
              {t('buttons.cancel')}
            </Button>
            <Button
              onClick={handleImport}
              disabled={
                selectedKeys.size === 0 ||
                importing ||
                loadingPreview ||
                !!previewFailure ||
                !!successMessage
              }
            >
              {importing ? (
                <>
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  {t('kanban.trackerImport.importing')}
                </>
              ) : (
                t('kanban.trackerImport.importButton', {
                  count: selectedKeys.size,
                })
              )}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }
);

export const ImportFromTrackerDialog = defineModal<
  ImportFromTrackerDialogProps,
  ImportFromTrackerDialogResult
>(ImportFromTrackerDialogImpl);
