import { useTranslation } from 'react-i18next';
import {
  IssueWorkspaceCard,
  IssueWorkspaceCreateCard,
  type WorkspaceWithStats,
} from './IssueWorkspaceCard';
import type { SectionAction } from './CollapsibleSectionHeader';
import { IssuePanelSection } from './IssuePanelSection';

export interface IssueWorkspacesSectionProps {
  workspaces: WorkspaceWithStats[];
  isLoading?: boolean;
  actions?: SectionAction[];
  onWorkspaceClick?: (localWorkspaceId: string | null) => void;
  onCreateWorkspace?: () => void;
  onUnlinkWorkspace?: (localWorkspaceId: string) => void;
  onDeleteWorkspace?: (localWorkspaceId: string) => void;
  shouldAnimateCreateButton?: boolean;
}

/**
 * "Sessions on this card" in the card panel: one row per session (a
 * workspace in code), newest work first as the container orders them.
 */
export function IssueWorkspacesSection({
  workspaces,
  isLoading,
  actions = [],
  onWorkspaceClick,
  onCreateWorkspace,
  onUnlinkWorkspace,
  onDeleteWorkspace,
  shouldAnimateCreateButton = false,
}: IssueWorkspacesSectionProps) {
  const { t } = useTranslation('common');

  return (
    <IssuePanelSection
      label={t('kanban.sessions.title')}
      persistKey="kanban-issue-workspaces"
      collapsible
      actions={actions}
    >
      <div className="flex flex-col gap-2">
        {isLoading ? (
          <p className="m-0 text-[13px] text-low">{t('workspaces.loading')}</p>
        ) : workspaces.length === 0 ? (
          <IssueWorkspaceCreateCard
            onClick={onCreateWorkspace}
            shouldAnimateCreateButton={shouldAnimateCreateButton}
          />
        ) : (
          workspaces.map((workspace) => {
            const { localWorkspaceId } = workspace;
            return (
              <IssueWorkspaceCard
                key={workspace.id}
                workspace={workspace}
                showNoPrText={false}
                onClick={
                  onWorkspaceClick &&
                  localWorkspaceId &&
                  workspace.isOwnedByCurrentUser
                    ? () => onWorkspaceClick(localWorkspaceId)
                    : undefined
                }
                onUnlink={
                  onUnlinkWorkspace && localWorkspaceId
                    ? () => onUnlinkWorkspace(localWorkspaceId)
                    : undefined
                }
                onDelete={
                  onDeleteWorkspace &&
                  localWorkspaceId &&
                  workspace.isOwnedByCurrentUser
                    ? () => onDeleteWorkspace(localWorkspaceId)
                    : undefined
                }
              />
            );
          })
        )}
      </div>
    </IssuePanelSection>
  );
}
