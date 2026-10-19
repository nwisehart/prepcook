import { useMemo, useState, type ReactElement } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { Button } from '@prepcook/ui/components/Button';
import {
  PROJECT_ISSUES_SHAPE,
  PROJECT_PROJECT_STATUSES_SHAPE,
  type Issue,
} from 'shared/remote-types';
import { kbApi, type KbDocumentSummaryT } from '@/shared/lib/api';
import { useShape } from '@/shared/integrations/electric/hooks';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import {
  KnowledgeHeader,
  type ExportFormat,
  type KnowledgeViewMode,
} from './KnowledgeHeader';
import { KnowledgeTotals, type StatTile } from './KnowledgeTotals';
import { KnowledgeNodeRanking } from './KnowledgeNodeRanking';
import { KnowledgeRcaList, type RcaListItem } from './KnowledgeRcaList';
import { KnowledgeWeb } from './KnowledgeWeb';
import {
  calendarDaysAgo,
  countUnlinked,
  draftDocumentIds,
  humanizeTagValue,
  isInDayRange,
  nodeStats,
  rankedNodes,
  sortByNewest,
  tagValueCounts,
  type DayRange,
} from './knowledgeStats';

const KIND_ORDER = [
  'root_cause_class',
  'product_area',
  'component',
  'data_model',
  'infra',
] as const;

const ROOT_CAUSE_CLASS = 'root_cause_class';

/** RCA cards shown in the right column before "All N" expands it. */
const RECENT_LIMIT = 5;

/**
 * Knowledge (RCA graph) for one project: the RCA count and tag tiles, the
 * node ranking or force web, and the recent RCAs or one node's RCAs. The date
 * range scopes all of it and the export alike. Rendered as a full page (see
 * KnowledgePage); fetches on mount.
 */
export function KnowledgeView({ projectId }: { projectId: string }) {
  const { t, i18n } = useTranslation('common');
  const { config } = useUserSystem();

  // Whole-day UTC bounds on created_at; empty = unbounded.
  const [range, setRange] = useState<DayRange>({ since: '', until: '' });
  const [exportFormat, setExportFormat] = useState<ExportFormat>('json');
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [view, setView] = useState<KnowledgeViewMode>('ranking');
  const [showAllRecent, setShowAllRecent] = useState(false);

  const handleExport = () => {
    const params = new URLSearchParams({ project_id: projectId });
    if (range.since) params.set('since', range.since);
    if (range.until) params.set('until', range.until);
    params.set('format', exportFormat);
    const link = document.createElement('a');
    link.href = `/api/knowledge/export?${params.toString()}`;
    link.download = '';
    document.body.appendChild(link);
    link.click();
    link.remove();
  };

  const nodesQuery = useQuery({
    queryKey: ['kbNodes', projectId],
    queryFn: () => kbApi.listNodes(projectId),
  });
  const documentsQuery = useQuery({
    queryKey: ['kbDocuments', projectId],
    queryFn: () => kbApi.listDocuments(projectId, 'rca'),
  });
  const edgesQuery = useQuery({
    queryKey: ['kbEdges', projectId],
    queryFn: () => kbApi.listEdges(projectId),
  });
  // The board's cards, for draft state and board keys. Optional: until they
  // sync (or if they cannot), RCAs simply show as filed.
  const { data: issues } = useShape(PROJECT_ISSUES_SHAPE, {
    project_id: projectId,
  });
  const { data: statuses } = useShape(PROJECT_PROJECT_STATUSES_SHAPE, {
    project_id: projectId,
  });

  const nodes = useMemo(() => nodesQuery.data?.nodes ?? [], [nodesQuery.data]);
  const allDocuments = useMemo(
    () => documentsQuery.data?.documents ?? [],
    [documentsQuery.data]
  );
  const edges = useMemo(() => edgesQuery.data?.edges ?? [], [edgesQuery.data]);

  const documents = useMemo(
    () => allDocuments.filter((doc) => isInDayRange(doc.created_at, range)),
    [allDocuments, range]
  );
  const documentsById = useMemo(
    () => new Map(documents.map((doc) => [doc.id, doc])),
    [documents]
  );
  const issuesById = useMemo(
    () => new Map<string, Issue>(issues.map((issue) => [issue.id, issue])),
    [issues]
  );

  const stats = useMemo(
    () => nodeStats(nodes, edges, documentsById),
    [nodes, edges, documentsById]
  );
  const counts = useMemo(
    () => new Map([...stats.values()].map((s) => [s.node.id, s.count])),
    [stats]
  );
  const rankingsByKind = useMemo(
    () => KIND_ORDER.map((kind) => ({ kind, stats: rankedNodes(stats, kind) })),
    [stats]
  );
  const classRanking =
    rankingsByKind.find((ranking) => ranking.kind === ROOT_CAUSE_CLASS)
      ?.stats ?? [];
  const otherRankings = rankingsByKind.filter(
    (ranking) => ranking.kind !== ROOT_CAUSE_CLASS && ranking.stats.length > 0
  );
  const legendKinds = rankingsByKind
    .filter((ranking) => ranking.stats.length > 0)
    .map((ranking) => ranking.kind);
  const unclassifiedCount = useMemo(
    () => countUnlinked(documents, edges, nodes, ROOT_CAUSE_CLASS),
    [documents, edges, nodes]
  );
  const drafts = useMemo(
    () => draftDocumentIds(documents, issuesById, statuses),
    [documents, issuesById, statuses]
  );

  const tile = (
    id: string,
    label: string,
    namespace: string,
    order: 'count' | 'value' = 'count'
  ): StatTile => ({
    id,
    label,
    rows: tagValueCounts(documents, namespace, order).map((row) => ({
      key: row.value,
      label: humanizeTagValue(row.value),
      count: row.count,
    })),
  });
  const primaryTiles = [
    tile('origin', t('kanban.knowledge.origin', 'Origin'), 'origin'),
    tile(
      'detectable-by',
      t('kanban.knowledge.detectableBy', 'Could have been caught by'),
      'detectable-by'
    ),
    // p0…p3 read best in priority order, not by count.
    tile(
      'severity',
      t('kanban.knowledge.severity', 'Severity'),
      'severity',
      'value'
    ),
    tile('fix-type', t('kanban.knowledge.fixType', 'Fix type'), 'fix-type'),
  ];
  // Regression attribution: culprit:orchestrator vs culprit:human. Only set on
  // RCAs already tagged origin:regression, so this reads as "of the regressions
  // we attributed, how many were the orchestrator's own commits."
  const secondaryTiles = [
    tile('area', t('kanban.knowledge.area', 'Product area'), 'area'),
    tile(
      'culprit',
      t('kanban.knowledge.regressionCulprit', 'Regression introduced by'),
      'culprit'
    ),
  ].filter((secondary) => secondary.rows.length > 0);

  const ticketing = config?.ticketing;
  const jiraSite = ticketing?.site?.trim() || 'example.atlassian.net';
  // The ticket's page on the configured source; keys end in the issue
  // number on GitHub Issues and are incident ids on PagerDuty.
  const jiraUrl = (key: string) => {
    const repo = ticketing?.repo
      ?.trim()
      .replace(/^https?:\/\//, '')
      .replace(/\.git$/, '')
      .replace(/\/$/, '');
    if (ticketing?.kind === 'github_issues' && repo) {
      const base =
        repo.split('/').length === 3
          ? `https://${repo}`
          : `https://github.com/${repo}`;
      return `${base}/issues/${key.split(/[-#]/).pop()}`;
    }
    if (ticketing?.kind === 'pagerduty') {
      return `https://app.pagerduty.com/incidents/${key}`;
    }
    return `https://${jiraSite}/browse/${key}`;
  };
  const openDocument = (documentId: string) => {
    window.open(`/api/knowledge/documents/${documentId}/html`, '_blank');
  };
  const cardKey = (doc: KbDocumentSummaryT): string | null =>
    doc.jira_key ??
    (doc.issue_id ? issuesById.get(doc.issue_id)?.simple_id : null) ??
    null;

  const kindLabel = (kind: string): string => {
    switch (kind) {
      case 'root_cause_class':
        return t('kanban.knowledge.kinds.rootCauseClass', 'Root-cause classes');
      case 'product_area':
        return t('kanban.knowledge.kinds.productArea', 'Product areas');
      case 'component':
        return t('kanban.knowledge.kinds.component', 'Components');
      case 'data_model':
        return t('kanban.knowledge.kinds.dataModel', 'Data models');
      case 'infra':
        return t('kanban.knowledge.kinds.infra', 'Infrastructure');
      default:
        return kind;
    }
  };

  const formatDay = (day: string) => {
    const date = new Date(`${day}T00:00:00Z`);
    if (Number.isNaN(date.getTime())) return day;
    return date.toLocaleDateString(i18n.language, {
      month: 'short',
      day: 'numeric',
      year:
        date.getUTCFullYear() === new Date().getUTCFullYear()
          ? undefined
          : 'numeric',
      timeZone: 'UTC',
    });
  };
  const rangeLabel =
    range.since && range.until
      ? t('kanban.knowledge.range.between', '{{from}} – {{to}}', {
          from: formatDay(range.since),
          to: formatDay(range.until),
        })
      : range.since
        ? t('kanban.knowledge.range.since', 'Since {{date}}', {
            date: formatDay(range.since),
          })
        : range.until
          ? t('kanban.knowledge.range.until', 'Until {{date}}', {
              date: formatDay(range.until),
            })
          : t('kanban.knowledge.range.allTime', 'All time');

  const now = new Date();
  const filedLabel = (timestamp: string) => {
    const days = calendarDaysAgo(timestamp, now);
    if (days <= 0) return t('kanban.knowledge.relative.today', 'today');
    if (days === 1)
      return t('kanban.knowledge.relative.yesterday', 'yesterday');
    if (days < 7)
      return t('kanban.knowledge.relative.daysAgo', '{{days}} d ago', {
        days,
      });
    const date = new Date(timestamp);
    return date.toLocaleDateString(i18n.language, {
      month: 'short',
      day: 'numeric',
      year: date.getFullYear() === now.getFullYear() ? undefined : 'numeric',
    });
  };
  const toListItem = (
    doc: KbDocumentSummaryT,
    key: string,
    note?: string | null
  ): RcaListItem => ({
    key,
    doc,
    cardKey: cardKey(doc),
    keyUrl: doc.jira_key ? jiraUrl(doc.jira_key) : null,
    isDraft: drafts.has(doc.id),
    filedLabel: filedLabel(doc.created_at),
    filedTitle: new Date(doc.created_at).toLocaleString(i18n.language),
    note,
  });

  const selectedNode = useMemo(
    () => nodes.find((node) => node.id === selectedNodeId) ?? null,
    [nodes, selectedNodeId]
  );
  const newestDocuments = useMemo(() => sortByNewest(documents), [documents]);

  let rcaColumn: ReactElement;
  if (selectedNode) {
    const linked = edges
      .filter((edge) => edge.node_id === selectedNode.id)
      .map((edge) => ({ edge, doc: documentsById.get(edge.document_id) }))
      .filter(
        (entry): entry is typeof entry & { doc: KbDocumentSummaryT } =>
          entry.doc != null
      )
      .sort(
        (a, b) => Date.parse(b.doc.created_at) - Date.parse(a.doc.created_at)
      );
    rcaColumn = (
      <KnowledgeRcaList
        heading={selectedNode.name}
        action={{
          label: t('kanban.knowledge.allRcas', 'All RCAs'),
          onClick: () => setSelectedNodeId(null),
        }}
        description={selectedNode.description}
        items={linked.map(({ edge, doc }) =>
          toListItem(
            doc,
            edge.id,
            [
              edge.rationale,
              edge.confidence &&
                t('kanban.knowledge.linkConfidence', '{{level}} confidence', {
                  level: edge.confidence,
                }),
            ]
              .filter(Boolean)
              .join(' · ') || null
          )
        )}
        emptyLabel={t(
          'kanban.knowledge.noLinkedRcas',
          'No RCAs linked to this in the selected range.'
        )}
        onOpenDocument={openDocument}
      />
    );
  } else {
    const canExpand = newestDocuments.length > RECENT_LIMIT;
    const shown =
      showAllRecent || !canExpand
        ? newestDocuments
        : newestDocuments.slice(0, RECENT_LIMIT);
    rcaColumn = (
      <KnowledgeRcaList
        heading={t('kanban.knowledge.recentRcas', 'Recent RCAs')}
        action={
          canExpand
            ? {
                label: showAllRecent
                  ? t('kanban.knowledge.showRecent', 'Recent only')
                  : t('kanban.knowledge.showAll', 'All {{count}}', {
                      count: newestDocuments.length,
                    }),
                onClick: () => setShowAllRecent((current) => !current),
              }
            : null
        }
        items={shown.map((doc) => toListItem(doc, doc.id))}
        emptyLabel={t(
          'kanban.knowledge.emptyRange',
          'No RCAs were filed in this range.'
        )}
        onOpenDocument={openDocument}
      />
    );
  }

  const isLoading =
    nodesQuery.isLoading || documentsQuery.isLoading || edgesQuery.isLoading;
  const isError =
    nodesQuery.isError || documentsQuery.isError || edgesQuery.isError;
  const retry = () => {
    void nodesQuery.refetch();
    void documentsQuery.refetch();
    void edgesQuery.refetch();
  };

  let body: ReactElement;
  if (isLoading) {
    body = (
      <p className="py-12 text-center text-sm text-low">
        {t('states.loading')}
      </p>
    );
  } else if (isError) {
    body = (
      <div className="flex flex-col items-center gap-3 rounded-lg border bg-secondary px-4 py-10 text-center">
        <p className="text-sm text-error">
          {t(
            'kanban.knowledge.loadError',
            'The knowledge store could not be loaded.'
          )}
        </p>
        <Button variant="secondary" onClick={retry}>
          {t('buttons.retry')}
        </Button>
      </div>
    );
  } else if (allDocuments.length === 0) {
    body = (
      <p className="rounded-lg border bg-secondary px-4 py-10 text-center text-sm text-low">
        {t(
          'kanban.knowledge.empty',
          'No RCAs stored yet. Approved bug RCAs are filed here automatically.'
        )}
      </p>
    );
  } else {
    body = (
      <>
        <KnowledgeTotals
          total={documents.length}
          footnote={
            drafts.size > 0
              ? t('kanban.knowledge.draftsAtGate', {
                  count: drafts.size,
                  defaultValue: '{{count}} drafts at the gate',
                })
              : null
          }
          primaryTiles={primaryTiles}
          secondaryTiles={secondaryTiles}
        />

        <div className="flex flex-1 flex-wrap items-stretch gap-5">
          <div className="flex min-w-0 flex-[999_1_560px] flex-col gap-3">
            {view === 'web' ? (
              <KnowledgeWeb
                nodes={nodes}
                documents={documents}
                edges={edges}
                counts={counts}
                legendKinds={legendKinds}
                kindLabel={kindLabel}
                title={t('kanban.knowledge.webTitle', 'Graph')}
                hint={t(
                  'kanban.knowledge.webHint',
                  'Select a node to see its RCAs'
                )}
                selectedNodeId={selectedNodeId}
                onSelectNode={(nodeId) =>
                  setSelectedNodeId((current) =>
                    current === nodeId ? null : nodeId
                  )
                }
              />
            ) : (
              <>
                <KnowledgeNodeRanking
                  // Alone, the card stretches to the column height with its
                  // footer at the bottom; above other kinds it keeps its own
                  // height so the spare space is not trapped inside it.
                  className={otherRankings.length === 0 ? 'flex-1' : undefined}
                  title={kindLabel(ROOT_CAUSE_CLASS)}
                  subtitle={t(
                    'kanban.knowledge.classesSubtitle',
                    'RCAs per class · most recent card'
                  )}
                  stats={classRanking}
                  selectedNodeId={selectedNodeId}
                  onSelectNode={setSelectedNodeId}
                  cardKey={cardKey}
                  ticketUrl={jiraUrl}
                  emptyLabel={
                    documents.length === 0
                      ? t(
                          'kanban.knowledge.emptyRange',
                          'No RCAs were filed in this range.'
                        )
                      : t(
                          'kanban.knowledge.noClasses',
                          'No RCA is linked to a root-cause class yet. Run Analyze RCAs from the board menu to link them.'
                        )
                  }
                  footer={
                    classRanking.length > 0 && unclassifiedCount > 0 ? (
                      <span>
                        {t('kanban.knowledge.unclassified', {
                          count: unclassifiedCount,
                          defaultValue: '{{count}} RCAs have no class yet.',
                        })}
                      </span>
                    ) : null
                  }
                />
                {otherRankings.map((ranking) => (
                  <KnowledgeNodeRanking
                    key={ranking.kind}
                    title={kindLabel(ranking.kind)}
                    subtitle={t(
                      'kanban.knowledge.nodesSubtitle',
                      'RCAs linked · most recent card'
                    )}
                    stats={ranking.stats}
                    selectedNodeId={selectedNodeId}
                    onSelectNode={setSelectedNodeId}
                    cardKey={cardKey}
                    ticketUrl={jiraUrl}
                  />
                ))}
              </>
            )}
          </div>
          {rcaColumn}
        </div>
      </>
    );
  }

  return (
    <div className="flex flex-1 flex-col gap-5">
      <KnowledgeHeader
        range={range}
        rangeLabel={rangeLabel}
        onRangeChange={setRange}
        view={view}
        onViewChange={setView}
        exportFormat={exportFormat}
        onExportFormatChange={setExportFormat}
        onExport={handleExport}
      />
      {body}
    </div>
  );
}
