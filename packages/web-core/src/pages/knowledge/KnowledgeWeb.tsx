import { useMemo, type CSSProperties } from 'react';
import type { KbDocumentSummaryT, KbEdgeT, KbNodeT } from '@/shared/lib/api';

interface WebPoint {
  x: number;
  y: number;
}

/**
 * Node kinds are told apart by fill step and stroke, all from the brand hue
 * and the neutrals: status colors (success, error, attention) keep their own
 * meaning elsewhere in the app and are never reused as categories.
 */
const KIND_STYLES: Record<
  string,
  { fill: string; stroke: string; dash?: string }
> = {
  root_cause_class: {
    fill: 'hsl(var(--brand))',
    stroke: 'hsl(var(--brand))',
  },
  product_area: {
    fill: 'hsl(var(--brand-soft))',
    stroke: 'hsl(var(--brand))',
  },
  component: {
    fill: 'hsl(var(--bg-panel))',
    stroke: 'hsl(var(--text-low))',
  },
  data_model: {
    fill: 'hsl(var(--bg-secondary))',
    stroke: 'hsl(var(--text-low))',
  },
  infra: {
    fill: 'hsl(var(--bg-secondary))',
    stroke: 'hsl(var(--text-low))',
    dash: '3 2',
  },
};

const FALLBACK_STYLE = KIND_STYLES.component;

const WIDTH = 460;
const HEIGHT = 420;

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

function legendSwatchStyle(kind: string): CSSProperties {
  const style = KIND_STYLES[kind] ?? FALLBACK_STYLE;
  return {
    backgroundColor: style.fill,
    borderColor: style.stroke,
    borderStyle: style.dash ? 'dashed' : 'solid',
  };
}

interface KnowledgeWebProps {
  nodes: KbNodeT[];
  documents: KbDocumentSummaryT[];
  edges: KbEdgeT[];
  /** RCAs per node within the current range; nodes at 0 are left out. */
  counts: Map<string, number>;
  /** Kinds to list in the legend, in display order. */
  legendKinds: string[];
  kindLabel: (kind: string) => string;
  title: string;
  hint: string;
  selectedNodeId: string | null;
  onSelectNode: (nodeId: string) => void;
}

/** The RCA graph as a force-laid web: concept nodes and the RCAs they link. */
export function KnowledgeWeb({
  nodes,
  documents,
  edges,
  counts,
  legendKinds,
  kindLabel,
  title,
  hint,
  selectedNodeId,
  onSelectNode,
}: KnowledgeWebProps) {
  const { positions, concepts, docs, links } = useMemo(() => {
    const concepts = nodes.filter((node) => (counts.get(node.id) ?? 0) > 0);
    const conceptIds = new Set(concepts.map((node) => node.id));
    const documentIds = new Set(documents.map((doc) => doc.id));
    const visibleEdges = edges.filter(
      (edge) =>
        conceptIds.has(edge.node_id) && documentIds.has(edge.document_id)
    );
    const linkedDocIds = new Set(visibleEdges.map((edge) => edge.document_id));
    const docs = documents.filter((doc) => linkedDocIds.has(doc.id));
    const links: Array<[string, string]> = visibleEdges.map((edge) => [
      edge.node_id,
      edge.document_id,
    ]);
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
  }, [nodes, documents, edges, counts]);

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
    <section
      aria-label={title}
      className="flex min-w-0 flex-col gap-3 rounded-lg border bg-secondary px-5 py-3.5"
    >
      <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
        <h2 className="text-sm font-bold text-high">{title}</h2>
        <span className="text-[12.5px] text-low">{hint}</span>
      </div>
      <svg
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        className="mx-auto block h-auto w-full max-w-[640px]"
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
              stroke="hsl(var(--text-low))"
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
              fill="hsl(var(--text-low))"
              fillOpacity={dimmed ? 0.25 : 0.8}
            >
              <title>{doc.jira_key ?? doc.title}</title>
            </circle>
          );
        })}
        {concepts.map((node) => {
          const point = positions.get(node.id);
          if (!point) return null;
          const count = counts.get(node.id) ?? 0;
          const radius = 8 + Math.min(count, 12) * 1.5;
          const selected = node.id === selectedNodeId;
          const dimmed = highlighted != null && !selected;
          const style = KIND_STYLES[node.kind] ?? FALLBACK_STYLE;
          return (
            <g
              key={node.id}
              role="button"
              tabIndex={0}
              aria-pressed={selected}
              aria-label={`${node.name} (${count})`}
              className="cursor-pointer focus:outline-none"
              onClick={() => onSelectNode(node.id)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' || event.key === ' ') {
                  event.preventDefault();
                  onSelectNode(node.id);
                }
              }}
            >
              {node.investigation_jira_key && (
                <circle
                  cx={point.x}
                  cy={point.y}
                  r={radius + 3.5}
                  fill="none"
                  stroke="hsl(var(--brand-secondary))"
                  strokeDasharray="2 2"
                  strokeOpacity={dimmed ? 0.3 : 1}
                />
              )}
              <circle
                cx={point.x}
                cy={point.y}
                r={radius}
                fill={style.fill}
                fillOpacity={dimmed ? 0.3 : 1}
                stroke={selected ? 'hsl(var(--text-high))' : style.stroke}
                strokeOpacity={dimmed ? 0.3 : 1}
                strokeWidth={selected ? 2.5 : 1.5}
                strokeDasharray={selected ? undefined : style.dash}
              >
                <title>
                  {node.investigation_jira_key
                    ? `${node.name} (${count}) · ${node.investigation_jira_key}`
                    : `${node.name} (${count})`}
                </title>
              </circle>
              <text
                x={point.x}
                y={point.y - radius - 5}
                textAnchor="middle"
                fontSize={9}
                fill="hsl(var(--text-normal))"
                fillOpacity={dimmed ? 0.35 : 1}
                pointerEvents="none"
              >
                {node.name.length > 24
                  ? `${node.name.slice(0, 22)}…`
                  : node.name}
              </text>
            </g>
          );
        })}
      </svg>
      {legendKinds.length > 0 && (
        <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-low">
          {legendKinds.map((kind) => (
            <span key={kind} className="inline-flex items-center gap-1.5">
              <span
                className="inline-block size-2.5 rounded-full border"
                style={legendSwatchStyle(kind)}
              />
              {kindLabel(kind)}
            </span>
          ))}
        </div>
      )}
    </section>
  );
}
