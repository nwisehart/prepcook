import { useMemo, useCallback } from 'react';
import { useParams } from '@tanstack/react-router';
import {
  PlusIcon,
  ArrowBendUpRightIcon,
  ProhibitIcon,
  ArrowsLeftRightIcon,
  CopyIcon,
} from '@phosphor-icons/react';
import { useProjectContext } from '@/shared/hooks/useProjectContext';
import { useActions } from '@/shared/hooks/useActions';
import { useAppNavigation } from '@/shared/hooks/useAppNavigation';
import { resolveRelationshipsForIssue } from '@/shared/lib/resolveRelationships';
import { IssueRelationshipsSection } from '@prepcook/ui/components/IssueRelationshipsSection';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from '@prepcook/ui/components/Dropdown';

interface IssueRelationshipsSectionContainerProps {
  issueId: string;
}

export function IssueRelationshipsSectionContainer({
  issueId,
}: IssueRelationshipsSectionContainerProps) {
  const { projectId } = useParams({ strict: false });
  const appNavigation = useAppNavigation();
  const { openRelationshipSelection } = useActions();

  const {
    getRelationshipsForIssue,
    removeIssueRelationship,
    issuesById,
    isLoading,
  } = useProjectContext();

  const relationships = useMemo(
    () =>
      resolveRelationshipsForIssue(
        issueId,
        getRelationshipsForIssue(issueId),
        issuesById
      ).map((relationship) => ({
        ...relationship,
        relatedIssueTitle:
          issuesById.get(relationship.relatedIssueId)?.title ?? null,
      })),
    [issueId, getRelationshipsForIssue, issuesById]
  );

  const handleRelationshipClick = useCallback(
    (relatedIssueId: string) => {
      if (!projectId) {
        return;
      }

      appNavigation.goToProjectIssue(projectId, relatedIssueId);
    },
    [projectId, appNavigation]
  );

  const handleRemoveRelationship = useCallback(
    (relationshipId: string) => {
      removeIssueRelationship(relationshipId);
    },
    [removeIssueRelationship]
  );

  const handleSelectType = useCallback(
    (
      relationshipType: 'blocking' | 'related' | 'has_duplicate',
      direction: 'forward' | 'reverse'
    ) => {
      if (projectId) {
        openRelationshipSelection(
          projectId,
          issueId,
          relationshipType,
          direction
        );
      }
    },
    [projectId, issueId, openRelationshipSelection]
  );

  const headerExtra = (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          className="flex size-6 items-center justify-center rounded-sm text-low transition-colors hover:bg-panel hover:text-high"
          onClick={(e) => e.stopPropagation()}
        >
          <PlusIcon className="size-3.5" weight="bold" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem
          icon={ArrowBendUpRightIcon}
          onSelect={() => handleSelectType('blocking', 'forward')}
        >
          Blocks...
        </DropdownMenuItem>
        <DropdownMenuItem
          icon={ProhibitIcon}
          onSelect={() => handleSelectType('blocking', 'reverse')}
        >
          Blocked by...
        </DropdownMenuItem>
        <DropdownMenuItem
          icon={ArrowsLeftRightIcon}
          onSelect={() => handleSelectType('related', 'forward')}
        >
          Related to...
        </DropdownMenuItem>
        <DropdownMenuItem
          icon={CopyIcon}
          onSelect={() => handleSelectType('has_duplicate', 'forward')}
        >
          Duplicate of...
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );

  return (
    <IssueRelationshipsSection
      relationships={relationships}
      onRelationshipClick={handleRelationshipClick}
      onRemoveRelationship={handleRemoveRelationship}
      isLoading={isLoading}
      headerExtra={headerExtra}
    />
  );
}
