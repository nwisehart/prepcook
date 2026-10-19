'use client';

import { useTranslation } from 'react-i18next';
import { XIcon } from '@phosphor-icons/react';
import { cn } from '../lib/cn';
import { IssuePanelSection } from './IssuePanelSection';
import {
  relationshipLabel,
  type RelationshipDisplayType,
} from './RelationshipBadge';

export interface IssueRelationshipsSectionRelationship {
  relationshipId: string;
  relatedIssueId: string;
  relatedIssueDisplayId: string;
  displayType: RelationshipDisplayType;
  /** The related card's title, when it is on this board. */
  relatedIssueTitle?: string | null;
}

export interface IssueRelationshipsSectionProps {
  relationships: IssueRelationshipsSectionRelationship[];
  onRelationshipClick: (relatedIssueId: string) => void;
  onRemoveRelationship?: (relationshipId: string) => void;
  isLoading?: boolean;
  headerExtra?: React.ReactNode;
}

/** "Related" in the card panel: one row per linked card. */
export function IssueRelationshipsSection({
  relationships,
  onRelationshipClick,
  onRemoveRelationship,
  isLoading,
  headerExtra,
}: IssueRelationshipsSectionProps) {
  const { t } = useTranslation('common');

  return (
    <IssuePanelSection
      label={t('kanban.panel.related')}
      persistKey="kanban-issue-relationships"
      collapsible
      headerExtra={headerExtra}
    >
      <div className="flex flex-col gap-1.5 text-[13px]">
        {isLoading ? (
          <p className="m-0 text-low">{t('states.loading')}</p>
        ) : relationships.length === 0 ? (
          <p className="m-0 text-low">
            {t('kanban.noRelationships', 'No relationships')}
          </p>
        ) : (
          relationships.map((rel) => {
            const isBlocking =
              rel.displayType === 'blocks' || rel.displayType === 'blocked_by';
            return (
              <div
                key={rel.relationshipId}
                className="group flex min-w-0 items-center gap-2"
              >
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    onRelationshipClick(rel.relatedIssueId);
                  }}
                  className="flex min-w-0 flex-1 items-center gap-2 rounded-sm text-left text-high hover:text-brand-secondary focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand"
                >
                  <span className="shrink-0 font-mono text-xs text-low">
                    {rel.relatedIssueDisplayId}
                  </span>
                  <span className="min-w-0 flex-1 truncate">
                    {rel.relatedIssueTitle ?? ''}
                  </span>
                  <span
                    className={cn(
                      'inline-flex h-[22px] shrink-0 items-center rounded-full px-2 text-xs font-medium',
                      isBlocking
                        ? 'bg-error-soft text-error'
                        : 'bg-panel text-normal'
                    )}
                  >
                    {relationshipLabel(rel.displayType)}
                  </span>
                </button>
                {onRemoveRelationship && (
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      onRemoveRelationship(rel.relationshipId);
                    }}
                    className="flex size-6 shrink-0 items-center justify-center rounded-sm text-low opacity-0 transition-opacity hover:bg-error-soft hover:text-error focus-visible:opacity-100 group-hover:opacity-100"
                    aria-label={t('kanban.panel.removeRelationship')}
                  >
                    <XIcon className="size-3" weight="bold" />
                  </button>
                )}
              </div>
            );
          })
        )}
      </div>
    </IssuePanelSection>
  );
}
