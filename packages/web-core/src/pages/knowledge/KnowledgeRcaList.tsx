import { useTranslation } from 'react-i18next';
import { cn } from '@/shared/lib/utils';
import type { KbDocumentSummaryT } from '@/shared/lib/api';

const CHIP_CLASS =
  'inline-flex h-[22px] items-center whitespace-nowrap rounded-full px-2 text-xs font-medium leading-none';

export interface RcaListItem {
  doc: KbDocumentSummaryT;
  /** Tracker key, else the board card's key; null when neither exists. */
  cardKey: string | null;
  /** Where the key links (the tracker ticket); null renders it as text. */
  keyUrl: string | null;
  isDraft: boolean;
  /** Short filing date ("today", "4 d ago", "Oct 2"). */
  filedLabel: string;
  /** Exact filing time, for the date's tooltip. */
  filedTitle: string;
  /** Extra muted line under the tags, e.g. why it is linked to a node. */
  note?: string | null;
  key: string;
}

function RcaCard({
  item,
  onOpen,
}: {
  item: RcaListItem;
  onOpen: (documentId: string) => void;
}) {
  const { t } = useTranslation('common');
  const { doc } = item;
  return (
    <article
      className={cn(
        'flex min-w-0 flex-col gap-2 rounded-lg border bg-secondary px-3.5 py-3',
        item.isDraft && 'border-attention-border'
      )}
    >
      <div className="flex min-w-0 items-center gap-2">
        {item.cardKey &&
          (item.keyUrl ? (
            <a
              href={item.keyUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="shrink-0 rounded-sm font-mono text-sm font-semibold text-brand-secondary hover:underline focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand"
            >
              {item.cardKey}
            </a>
          ) : (
            <span className="shrink-0 font-mono text-sm font-semibold text-high">
              {item.cardKey}
            </span>
          ))}
        {item.isDraft && (
          <span
            className={cn(
              CHIP_CLASS,
              'bg-attention-soft text-attention-foreground'
            )}
          >
            {t('kanban.knowledge.draftChip', 'draft · at the gate')}
          </span>
        )}
        <span
          className="ml-auto shrink-0 text-xs text-low"
          title={item.filedTitle}
        >
          {item.filedLabel}
        </span>
      </div>
      <button
        type="button"
        onClick={() => onOpen(doc.id)}
        className="rounded-sm text-left text-sm font-semibold text-high hover:underline focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand"
      >
        {doc.title}
      </button>
      {doc.summary && (
        <p className="truncate text-[12.5px] text-normal" title={doc.summary}>
          {doc.summary}
        </p>
      )}
      {doc.tags.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {doc.tags.map((tag) => (
            <span key={tag} className={cn(CHIP_CLASS, 'bg-panel text-normal')}>
              {tag}
            </span>
          ))}
        </div>
      )}
      {item.note && (
        <p className="border-t border-divider pt-2 text-xs text-low">
          {item.note}
        </p>
      )}
    </article>
  );
}

interface KnowledgeRcaListProps {
  heading: string;
  /** A link-styled action at the right of the heading ("All 43"). */
  action?: { label: string; onClick: () => void } | null;
  description?: string | null;
  items: RcaListItem[];
  emptyLabel: string;
  onOpenDocument: (documentId: string) => void;
}

/** The right-hand column: a heading and a stack of RCA cards. */
export function KnowledgeRcaList({
  heading,
  action,
  description,
  items,
  emptyLabel,
  onOpenDocument,
}: KnowledgeRcaListProps) {
  return (
    <section
      aria-label={heading}
      className="flex min-w-0 flex-[0_1_460px] flex-col gap-2.5"
    >
      <div className="flex min-w-0 items-baseline justify-between gap-3 px-0.5">
        <h2 className="truncate text-sm font-bold text-high">{heading}</h2>
        {action && (
          <button
            type="button"
            onClick={action.onClick}
            className="shrink-0 rounded-sm text-[12.5px] font-semibold text-brand-secondary hover:underline focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand"
          >
            {action.label}
          </button>
        )}
      </div>
      {description && (
        <p className="-mt-1 px-0.5 text-[12.5px] text-low">{description}</p>
      )}
      {items.length === 0 ? (
        <p className="rounded-lg border border-dashed px-3.5 py-3 text-[12.5px] text-low">
          {emptyLabel}
        </p>
      ) : (
        items.map((item) => (
          <RcaCard key={item.key} item={item} onOpen={onOpenDocument} />
        ))
      )}
    </section>
  );
}
