import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { cn } from '@/shared/lib/utils';
import type { KbDocumentSummaryT } from '@/shared/lib/api';
import type { NodeStat } from './knowledgeStats';

interface KnowledgeNodeRankingProps {
  title: string;
  subtitle: string;
  /** Ranked, most RCAs first; every entry has at least one RCA. */
  stats: NodeStat[];
  selectedNodeId: string | null;
  onSelectNode: (nodeId: string) => void;
  /** The key shown for an RCA's card (tracker key, else board key). */
  cardKey: (doc: KbDocumentSummaryT) => string | null;
  /** Where a pattern review's ticket opens. */
  ticketUrl: (key: string) => string;
  emptyLabel?: string;
  footer?: ReactNode;
  className?: string;
}

/**
 * One node kind ranked by RCA count: count, name, a brand bar scaled to the
 * top count, and the key of the most recently filed RCA. Each row selects its
 * node; the whole row is the hit target, with the pattern-review chip (when
 * the node has one) layered above it as its own link.
 */
export function KnowledgeNodeRanking({
  title,
  subtitle,
  stats,
  selectedNodeId,
  onSelectNode,
  cardKey,
  ticketUrl,
  emptyLabel,
  footer,
  className,
}: KnowledgeNodeRankingProps) {
  const { t } = useTranslation('common');
  const max = stats[0]?.count ?? 0;

  return (
    <section
      aria-label={title}
      className={cn(
        'flex min-w-0 flex-col gap-1 rounded-lg border bg-secondary px-2.5 pb-2.5 pt-3.5',
        className
      )}
    >
      <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-0.5 px-2.5 pb-2">
        <h2 className="text-sm font-bold text-high">{title}</h2>
        <span className="text-[12.5px] text-low">{subtitle}</span>
      </div>

      {stats.length === 0 && emptyLabel && (
        <p className="px-2.5 py-2 text-[12.5px] text-low">{emptyLabel}</p>
      )}

      {stats.map(({ node, count, latest }) => {
        const selected = node.id === selectedNodeId;
        const latestKey = latest ? cardKey(latest) : null;
        const width = max > 0 ? `${Math.max((count / max) * 100, 2)}%` : '0%';
        return (
          <div
            key={node.id}
            className={cn(
              'group relative grid grid-cols-[36px_minmax(0,1fr)_82px] items-center gap-3 rounded px-2.5 py-[9px] transition-colors hover:bg-panel md:grid-cols-[36px_minmax(0,2fr)_minmax(0,1fr)_82px]',
              selected && 'bg-panel'
            )}
          >
            <button
              type="button"
              aria-pressed={selected}
              title={node.name}
              onClick={() => onSelectNode(node.id)}
              className="absolute inset-0 rounded focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand"
            >
              <span className="sr-only">{`${node.name}, ${count}`}</span>
            </button>
            <span className="pointer-events-none font-mono text-sm font-medium text-high">
              {count}
            </span>
            <span className="pointer-events-none flex min-w-0 items-center gap-2">
              <span className="truncate text-sm font-medium text-high">
                {node.name}
              </span>
              {node.investigation_jira_key && (
                <a
                  href={ticketUrl(node.investigation_jira_key)}
                  target="_blank"
                  rel="noopener noreferrer"
                  title={t('kanban.knowledge.patternReview', 'Pattern review')}
                  className="pointer-events-auto relative z-10 inline-flex h-[22px] shrink-0 items-center rounded-full bg-brand-soft px-2 font-mono text-xs font-medium text-brand-soft-foreground hover:underline focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand"
                >
                  {node.investigation_jira_key}
                </a>
              )}
            </span>
            <span
              className={cn(
                'pointer-events-none hidden h-2 w-full max-w-[220px] justify-self-end overflow-hidden rounded-r-[4px] md:block',
                // The row turns bg-panel on hover and selection, so the track
                // steps to the surface color to stay visible against it.
                selected ? 'bg-secondary' : 'bg-panel group-hover:bg-secondary'
              )}
            >
              <span
                className="block h-full rounded-r-[4px] bg-brand"
                style={{ width }}
              />
            </span>
            <span className="pointer-events-none truncate text-right font-mono text-xs text-low">
              {latestKey}
            </span>
          </div>
        );
      })}

      {footer && (
        <div className="mt-auto flex items-center justify-between gap-3 px-2.5 pb-0.5 pt-2.5 text-[12.5px] text-low">
          {footer}
        </div>
      )}
    </section>
  );
}
