import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import {
  kbApi,
  type KbDocumentSummaryT,
  type KbEdgeT,
  type KbNodeT,
} from '@/shared/lib/api';
import { useUserSystem } from '@/shared/hooks/useUserSystem';

const KIND_ORDER = [
  'root_cause_class',
  'product_area',
  'component',
  'data_model',
  'infra',
] as const;

const KIND_LABELS: Record<string, string> = {
  root_cause_class: 'Root-cause classes',
  product_area: 'Product areas',
  component: 'Components',
  data_model: 'Data models',
  infra: 'Infrastructure',
};

/** Aggregates tag counts across documents, grouped by "namespace:" prefix. */
function tagCounts(documents: KbDocumentSummaryT[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const doc of documents) {
    for (const tag of doc.tags) {
      counts.set(tag, (counts.get(tag) ?? 0) + 1);
    }
  }
  return counts;
}

const KIND_COLORS: Record<string, string> = {
  root_cause_class: 'hsl(var(--error))',
  product_area: 'hsl(var(--brand))',
  component: 'hsl(var(--success))',
  data_model: 'hsl(var(--warning))',
  infra: 'hsl(var(--low))',
};

interface WebPoint {
  x: number;
  y: number;
}

/**
 * Tiny static force layout (repulsion + link springs + centering), computed
 * synchronously — the graphs here are dozens of nodes, so no dependency or
 * animation loop is warranted. Deterministic: initial positions come from
 * the node order, not randomness.
 */
function computeWebLayout(
  ids: string[],
  links: Array<[string, string]>,
  width: number,
  height: number
): Map<string, WebPoint> {
  const positions = new Map<string, WebPoint>();
  const cx = width / 2;
  const cy = height / 2;
  ids.forEach((id, index) => {
    const angle = (index / Math.max(ids.length, 1)) * Math.PI * 2;
    const radius = Math.min(width, height) * 0.35;
    positions.set(id, {
      x: cx + radius * Math.cos(angle),
      y: cy + radius * Math.sin(angle),
    });
  });

  const REPULSION = 4000;
  const SPRING = 0.02;
  const REST = 80;
  const CENTER_PULL = 0.01;

  for (let iteration = 0; iteration < 250; iteration++) {
    const forces = new Map<string, WebPoint>(
      ids.map((id) => [id, { x: 0, y: 0 }])
    );
    for (let i = 0; i < ids.length; i++) {
      for (let j = i + 1; j < ids.length; j++) {
        const a = positions.get(ids[i])!;
        const b = positions.get(ids[j])!;
        let dx = a.x - b.x;
        let dy = a.y - b.y;
        const distSq = Math.max(dx * dx + dy * dy, 25);
        const dist = Math.sqrt(distSq);
        dx /= dist;
        dy /= dist;
        const force = REPULSION / distSq;
        const fa = forces.get(ids[i])!;
        const fb = forces.get(ids[j])!;
        fa.x += dx * force;
        fa.y += dy * force;
        fb.x -= dx * force;
        fb.y -= dy * force;
      }
    }
    for (const [from, to] of links) {
      const a = positions.get(from);
      const b = positions.get(to);
      if (!a || !b) continue;
      const dx = b.x - a.x;
      const dy = b.y - a.y;
      const dist = Math.max(Math.sqrt(dx * dx + dy * dy), 1);
      const force = SPRING * (dist - REST);
      const fa = forces.get(from)!;
      const fb = forces.get(to)!;
      fa.x += (dx / dist) * force;
      fa.y += (dy / dist) * force;
      fb.x -= (dx / dist) * force;
      fb.y -= (dy / dist) * force;
    }
    const cooling = 1 - iteration / 250;
    for (const id of ids) {
      const point = positions.get(id)!;
      const force = forces.get(id)!;
      force.x += (cx - point.x) * CENTER_PULL;
      force.y += (cy - point.y) * CENTER_PULL;
      point.x += force.x * cooling;
      point.y += force.y * cooling;
      point.x = Math.min(Math.max(point.x, 20), width - 20);
      point.y = Math.min(Math.max(point.y, 20), height - 20);
    }
  }
  return positions;
}

function KnowledgeWeb({
  nodes,
  documents,
  edges,
  selectedNodeId,
  onSelectNode,
}: {
  nodes: KbNodeT[];
  documents: KbDocumentSummaryT[];
  edges: KbEdgeT[];
  selectedNodeId: string | null;
  onSelectNode: (nodeId: string) => void;
}) {
  const WIDTH = 460;
  const HEIGHT = 420;

  const { positions, concepts, docs, links } = useMemo(() => {
    const concepts = nodes.filter((node) => node.edge_count > 0);
    const conceptIds = new Set(concepts.map((node) => node.id));
    const linkedDocIds = new Set(
      edges
        .filter((edge) => conceptIds.has(edge.node_id))
        .map((edge) => edge.document_id)
    );
    const docs = documents.filter((doc) => linkedDocIds.has(doc.id));
    const links: Array<[string, string]> = edges
      .filter(
        (edge) =>
          conceptIds.has(edge.node_id) && linkedDocIds.has(edge.document_id)
      )
      .map((edge) => [edge.node_id, edge.document_id]);
    const ids = [
      ...concepts.map((node) => node.id),
      ...docs.map((doc) => doc.id),
    ];
    return {
      positions: computeWebLayout(ids, links, WIDTH, HEIGHT),
      concepts,
      docs,
      links,
    };
  }, [nodes, documents, edges]);

  const highlighted = useMemo(() => {
    if (!selectedNodeId) return null;
    const docIds = new Set(
      edges
        .filter((edge) => edge.node_id === selectedNodeId)
        .map((edge) => edge.document_id)
    );
    return { nodeId: selectedNodeId, docIds };
  }, [edges, selectedNodeId]);

  return (
    <svg
      viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
      className="w-full border rounded-sm bg-secondary/30"
    >
      {links.map(([from, to], index) => {
        const a = positions.get(from);
        const b = positions.get(to);
        if (!a || !b) return null;
        const dimmed =
          highlighted != null &&
          from !== highlighted.nodeId &&
          !highlighted.docIds.has(to);
        return (
          <line
            key={index}
            x1={a.x}
            y1={a.y}
            x2={b.x}
            y2={b.y}
            stroke="hsl(var(--low))"
            strokeOpacity={dimmed ? 0.12 : 0.4}
          />
        );
      })}
      {docs.map((doc) => {
        const point = positions.get(doc.id);
        if (!point) return null;
        const dimmed = highlighted != null && !highlighted.docIds.has(doc.id);
        return (
          <circle
            key={doc.id}
            cx={point.x}
            cy={point.y}
            r={4}
            fill="hsl(var(--low))"
            fillOpacity={dimmed ? 0.25 : 0.8}
          >
            <title>{doc.jira_key ?? doc.title}</title>
          </circle>
        );
      })}
      {concepts.map((node) => {
        const point = positions.get(node.id);
        if (!point) return null;
        const radius = 8 + Math.min(node.edge_count, 12) * 1.5;
        const dimmed = highlighted != null && node.id !== highlighted.nodeId;
        return (
          <g
            key={node.id}
            className="cursor-pointer"
            onClick={() => onSelectNode(node.id)}
          >
            <circle
              cx={point.x}
              cy={point.y}
              r={radius}
              fill={KIND_COLORS[node.kind] ?? 'hsl(var(--brand))'}
              fillOpacity={dimmed ? 0.25 : 0.85}
              stroke={
                node.id === selectedNodeId
                  ? 'hsl(var(--high))'
                  : node.investigation_jira_key
                    ? 'hsl(var(--warning))'
                    : 'none'
              }
              strokeWidth={2}
            >
              <title>{`${node.name} (${node.edge_count})`}</title>
            </circle>
            <text
              x={point.x}
              y={point.y - radius - 4}
              textAnchor="middle"
              fontSize={9}
              fill="hsl(var(--normal))"
              fillOpacity={dimmed ? 0.35 : 1}
              pointerEvents="none"
            >
              {node.name.length > 24 ? `${node.name.slice(0, 22)}…` : node.name}
            </text>
          </g>
        );
      })}
    </svg>
  );
}

/**
 * Knowledge (RCA graph) for one project: tag-count dashboard, node ranking /
 * force-web toggle, and per-node drill-down. Rendered as a full page (see
 * KnowledgePage); fetches on mount.
 */
export function KnowledgeView({ projectId }: { projectId: string }) {
  const { t } = useTranslation('common');
  // Export controls: whole-day UTC bounds on created_at; empty = unbounded.
  const [exportSince, setExportSince] = useState('');
  const [exportUntil, setExportUntil] = useState('');
  const [exportFormat, setExportFormat] = useState<'json' | 'csv'>('json');
  const handleExport = () => {
    const params = new URLSearchParams({ project_id: projectId });
    if (exportSince) params.set('since', exportSince);
    if (exportUntil) params.set('until', exportUntil);
    params.set('format', exportFormat);
    const link = document.createElement('a');
    link.href = `/api/knowledge/export?${params.toString()}`;
    link.download = '';
    document.body.appendChild(link);
    link.click();
    link.remove();
  };
  const { config } = useUserSystem();
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [view, setView] = useState<'ranking' | 'web'>('ranking');

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

  const nodes = useMemo(() => nodesQuery.data?.nodes ?? [], [nodesQuery.data]);
  const documents = useMemo(
    () => documentsQuery.data?.documents ?? [],
    [documentsQuery.data]
  );
  const edges = useMemo(() => edgesQuery.data?.edges ?? [], [edgesQuery.data]);

  const documentsById = useMemo(() => {
    const map = new Map<string, KbDocumentSummaryT>();
    for (const doc of documents) map.set(doc.id, doc);
    return map;
  }, [documents]);

  const nodesByKind = useMemo(() => {
    const map = new Map<string, KbNodeT[]>();
    for (const node of nodes) {
      const list = map.get(node.kind) ?? [];
      list.push(node);
      map.set(node.kind, list);
    }
    return map;
  }, [nodes]);

  const selectedNode = useMemo(
    () => nodes.find((node) => node.id === selectedNodeId) ?? null,
    [nodes, selectedNodeId]
  );
  const selectedNodeEdges = useMemo(
    () =>
      selectedNode
        ? edges.filter((edge: KbEdgeT) => edge.node_id === selectedNode.id)
        : [],
    [edges, selectedNode]
  );

  const counts = useMemo(() => tagCounts(documents), [documents]);
  const originCounts = useMemo(
    () =>
      [...counts.entries()]
        .filter(([tag]) => tag.startsWith('origin:'))
        .sort((a, b) => b[1] - a[1]),
    [counts]
  );
  const detectableCounts = useMemo(
    () =>
      [...counts.entries()]
        .filter(([tag]) => tag.startsWith('detectable-by:'))
        .sort((a, b) => b[1] - a[1]),
    [counts]
  );
  // Regression attribution: culprit:orchestrator vs culprit:human. Only set on
  // RCAs already tagged origin:regression, so this reads as "of the regressions
  // we attributed, how many were the orchestrator's own commits."
  const culpritCounts = useMemo(
    () =>
      [...counts.entries()]
        .filter(([tag]) => tag.startsWith('culprit:'))
        .sort((a, b) => b[1] - a[1]),
    [counts]
  );

  const areaCounts = useMemo(
    () =>
      [...counts.entries()]
        .filter(([tag]) => tag.startsWith('area:'))
        .sort((a, b) => b[1] - a[1]),
    [counts]
  );
  const fixTypeCounts = useMemo(
    () =>
      [...counts.entries()]
        .filter(([tag]) => tag.startsWith('fix-type:'))
        .sort((a, b) => b[1] - a[1]),
    [counts]
  );
  const severityCounts = useMemo(
    () =>
      [...counts.entries()]
        .filter(([tag]) => tag.startsWith('severity:'))
        .sort((a, b) => a[0].localeCompare(b[0])),
    [counts]
  );

  const jiraSite = config?.tracker?.site?.trim() || 'example.atlassian.net';
  const jiraUrl = (key: string) => `https://${jiraSite}/browse/${key}`;
  const openDocument = (documentId: string) => {
    window.open(`/api/knowledge/documents/${documentId}/html`, '_blank');
  };

  const isLoading =
    nodesQuery.isLoading || documentsQuery.isLoading || edgesQuery.isLoading;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-end gap-2 flex-wrap">
        <div className="flex items-center gap-1 text-xs text-low">
          <input
            type="date"
            value={exportSince}
            onChange={(event) => setExportSince(event.target.value)}
            aria-label={t('kanban.knowledge.exportFrom', 'Export from')}
            className="bg-transparent border rounded-sm px-1.5 py-1"
          />
          <span>–</span>
          <input
            type="date"
            value={exportUntil}
            onChange={(event) => setExportUntil(event.target.value)}
            aria-label={t('kanban.knowledge.exportTo', 'Export to')}
            className="bg-transparent border rounded-sm px-1.5 py-1"
          />
          <select
            value={exportFormat}
            onChange={(event) =>
              setExportFormat(event.target.value as 'json' | 'csv')
            }
            aria-label={t('kanban.knowledge.exportFormat', 'Export format')}
            className="bg-transparent border rounded-sm px-1.5 py-1"
          >
            <option value="json">JSON</option>
            <option value="csv">CSV</option>
          </select>
          <button
            type="button"
            onClick={handleExport}
            className="px-2 py-1 rounded-sm border text-brand hover:bg-brand/10 transition-colors"
          >
            {t('kanban.knowledge.export', 'Export')}
          </button>
        </div>
        <div className="flex rounded-sm border overflow-hidden text-xs">
          <button
            type="button"
            onClick={() => setView('ranking')}
            className={`px-2 py-1 transition-colors ${
              view === 'ranking'
                ? 'bg-brand/15 text-brand'
                : 'text-low hover:text-normal'
            }`}
          >
            {t('kanban.knowledge.viewRanking', 'Ranking')}
          </button>
          <button
            type="button"
            onClick={() => setView('web')}
            className={`px-2 py-1 transition-colors border-l ${
              view === 'web'
                ? 'bg-brand/15 text-brand'
                : 'text-low hover:text-normal'
            }`}
          >
            {t('kanban.knowledge.viewWeb', 'Web')}
          </button>
        </div>
      </div>

      {isLoading ? (
        <p className="text-low text-sm py-8 text-center">
          {t('states.loading')}
        </p>
      ) : documents.length === 0 ? (
        <p className="text-low text-sm py-8 text-center">
          {t(
            'kanban.knowledge.empty',
            'No RCAs stored yet. Approved bug RCAs are filed here automatically.'
          )}
        </p>
      ) : (
        <>
          {/* Process metrics: tag counts */}
          <div className="flex flex-wrap gap-4 text-sm">
            <div className="rounded-sm border px-3 py-2">
              <div className="text-low text-xs mb-1">
                {t('kanban.knowledge.rcaCount', 'RCAs stored')}
              </div>
              <div className="text-lg font-medium">{documents.length}</div>
            </div>
            {originCounts.length > 0 && (
              <div className="rounded-sm border px-3 py-2">
                <div className="text-low text-xs mb-1">
                  {t('kanban.knowledge.origin', 'Origin')}
                </div>
                <div className="flex gap-3">
                  {originCounts.map(([tag, count]) => (
                    <span key={tag}>
                      <span className="font-medium">{count}</span>{' '}
                      <span className="text-low">
                        {tag.replace('origin:', '')}
                      </span>
                    </span>
                  ))}
                </div>
              </div>
            )}
            {detectableCounts.length > 0 && (
              <div className="rounded-sm border px-3 py-2">
                <div className="text-low text-xs mb-1">
                  {t(
                    'kanban.knowledge.detectableBy',
                    'Could have been caught by'
                  )}
                </div>
                <div className="flex gap-3">
                  {detectableCounts.map(([tag, count]) => (
                    <span key={tag}>
                      <span className="font-medium">{count}</span>{' '}
                      <span className="text-low">
                        {tag.replace('detectable-by:', '')}
                      </span>
                    </span>
                  ))}
                </div>
              </div>
            )}
            {culpritCounts.length > 0 && (
              <div className="rounded-sm border px-3 py-2">
                <div className="text-low text-xs mb-1">
                  {t(
                    'kanban.knowledge.regressionCulprit',
                    'Regression introduced by'
                  )}
                </div>
                <div className="flex gap-3">
                  {culpritCounts.map(([tag, count]) => (
                    <span key={tag}>
                      <span className="font-medium">{count}</span>{' '}
                      <span className="text-low">
                        {tag.replace('culprit:', '')}
                      </span>
                    </span>
                  ))}
                </div>
              </div>
            )}
            {severityCounts.length > 0 && (
              <div className="rounded-sm border px-3 py-2">
                <div className="text-low text-xs mb-1">
                  {t('kanban.knowledge.severity', 'Severity')}
                </div>
                <div className="flex gap-3">
                  {severityCounts.map(([tag, count]) => (
                    <span key={tag}>
                      <span className="font-medium">{count}</span>{' '}
                      <span className="text-low">
                        {tag.replace('severity:', '')}
                      </span>
                    </span>
                  ))}
                </div>
              </div>
            )}
            {areaCounts.length > 0 && (
              <div className="rounded-sm border px-3 py-2">
                <div className="text-low text-xs mb-1">
                  {t('kanban.knowledge.area', 'Product area')}
                </div>
                <div className="flex gap-3 flex-wrap">
                  {areaCounts.map(([tag, count]) => (
                    <span key={tag}>
                      <span className="font-medium">{count}</span>{' '}
                      <span className="text-low">
                        {tag.replace('area:', '')}
                      </span>
                    </span>
                  ))}
                </div>
              </div>
            )}
            {fixTypeCounts.length > 0 && (
              <div className="rounded-sm border px-3 py-2">
                <div className="text-low text-xs mb-1">
                  {t('kanban.knowledge.fixType', 'Fix type')}
                </div>
                <div className="flex gap-3">
                  {fixTypeCounts.map(([tag, count]) => (
                    <span key={tag}>
                      <span className="font-medium">{count}</span>{' '}
                      <span className="text-low">
                        {tag.replace('fix-type:', '')}
                      </span>
                    </span>
                  ))}
                </div>
              </div>
            )}
          </div>

          <div className="grid grid-cols-2 gap-4">
            {/* Node ranking or force web */}
            {view === 'web' ? (
              <div>
                <KnowledgeWeb
                  nodes={nodes}
                  documents={documents}
                  edges={edges}
                  selectedNodeId={selectedNodeId}
                  onSelectNode={(nodeId) =>
                    setSelectedNodeId((current) =>
                      current === nodeId ? null : nodeId
                    )
                  }
                />
                <div className="flex flex-wrap gap-3 mt-2 text-xs text-low">
                  {KIND_ORDER.filter((kind) =>
                    (nodesByKind.get(kind) ?? []).some(
                      (node) => node.edge_count > 0
                    )
                  ).map((kind) => (
                    <span key={kind} className="flex items-center gap-1">
                      <span
                        className="inline-block w-2.5 h-2.5 rounded-full"
                        style={{ backgroundColor: KIND_COLORS[kind] }}
                      />
                      {KIND_LABELS[kind] ?? kind}
                    </span>
                  ))}
                </div>
              </div>
            ) : (
              <div className="space-y-3">
                {KIND_ORDER.filter((kind) =>
                  (nodesByKind.get(kind) ?? []).some(
                    (node) => node.edge_count > 0
                  )
                ).map((kind) => (
                  <div key={kind}>
                    <div className="text-xs text-low uppercase tracking-wide mb-1">
                      {KIND_LABELS[kind] ?? kind}
                    </div>
                    <div className="space-y-half">
                      {(nodesByKind.get(kind) ?? [])
                        .filter((node) => node.edge_count > 0)
                        .map((node) => (
                          <button
                            key={node.id}
                            type="button"
                            onClick={() => setSelectedNodeId(node.id)}
                            className={`w-full flex items-center gap-2 px-2 py-1 rounded-sm text-left text-sm transition-colors ${
                              selectedNodeId === node.id
                                ? 'bg-brand/10'
                                : 'hover:bg-secondary'
                            }`}
                          >
                            <span className="inline-flex items-center justify-center min-w-6 h-5 px-1 rounded-sm bg-panel text-xs font-medium">
                              {node.edge_count}
                            </span>
                            <span className="truncate flex-1">{node.name}</span>
                            {node.investigation_jira_key && (
                              <a
                                href={jiraUrl(node.investigation_jira_key)}
                                target="_blank"
                                rel="noopener noreferrer"
                                onClick={(e) => e.stopPropagation()}
                                className="shrink-0 px-1.5 py-0.5 rounded text-xs bg-warning/15 text-warning hover:bg-warning/25"
                              >
                                {node.investigation_jira_key}
                              </a>
                            )}
                          </button>
                        ))}
                    </div>
                  </div>
                ))}
              </div>
            )}

            {/* Detail: selected node's linked RCAs, or the full RCA list */}
            <div className="space-y-2">
              {selectedNode ? (
                <>
                  <div>
                    <div className="text-sm font-medium">
                      {selectedNode.name}
                    </div>
                    {selectedNode.description && (
                      <p className="text-xs text-low mt-half">
                        {selectedNode.description}
                      </p>
                    )}
                  </div>
                  {selectedNodeEdges.map((edge) => {
                    const doc = documentsById.get(edge.document_id);
                    if (!doc) return null;
                    return (
                      <div
                        key={edge.id}
                        className="rounded-sm border px-2 py-1.5 text-sm"
                      >
                        <div className="flex items-center gap-2">
                          {doc.jira_key && (
                            <a
                              href={jiraUrl(doc.jira_key)}
                              target="_blank"
                              rel="noopener noreferrer"
                              className="text-brand hover:underline shrink-0"
                            >
                              {doc.jira_key}
                            </a>
                          )}
                          <button
                            type="button"
                            onClick={() => openDocument(doc.id)}
                            className="truncate flex-1 text-left hover:underline"
                            title={doc.title}
                          >
                            {doc.summary ?? doc.title}
                          </button>
                          {edge.confidence && (
                            <span className="shrink-0 text-xs text-low">
                              {edge.confidence}
                            </span>
                          )}
                        </div>
                        {edge.rationale && (
                          <p className="text-xs text-low mt-half">
                            {edge.rationale}
                          </p>
                        )}
                      </div>
                    );
                  })}
                </>
              ) : (
                <>
                  <div className="text-xs text-low uppercase tracking-wide">
                    {t('kanban.knowledge.allRcas', 'All RCAs')}
                  </div>
                  {documents.map((doc) => (
                    <div
                      key={doc.id}
                      className="rounded-sm border px-2 py-1.5 text-sm"
                    >
                      <div className="flex items-center gap-2">
                        {doc.jira_key && (
                          <a
                            href={jiraUrl(doc.jira_key)}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="text-brand hover:underline shrink-0"
                          >
                            {doc.jira_key}
                          </a>
                        )}
                        <button
                          type="button"
                          onClick={() => openDocument(doc.id)}
                          className="truncate flex-1 text-left hover:underline"
                          title={doc.title}
                        >
                          {doc.summary ?? doc.title}
                        </button>
                      </div>
                      {doc.tags.length > 0 && (
                        <div className="flex flex-wrap gap-1 mt-half">
                          {doc.tags.map((tag) => (
                            <span
                              key={tag}
                              className="px-1.5 py-0.5 rounded-sm bg-panel text-xs text-low"
                            >
                              {tag}
                            </span>
                          ))}
                        </div>
                      )}
                    </div>
                  ))}
                </>
              )}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
