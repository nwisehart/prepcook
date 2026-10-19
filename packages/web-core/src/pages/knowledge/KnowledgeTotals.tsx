import { useTranslation } from 'react-i18next';
import { cn } from '@/shared/lib/utils';

export interface StatTileRow {
  key: string;
  label: string;
  count: number;
}

export interface StatTile {
  id: string;
  label: string;
  rows: StatTileRow[];
}

/** Mini rows shown per tile; the rest are summarised as "+N more". */
const MAX_TILE_ROWS = 4;

const TILE_CLASS =
  'flex min-w-0 flex-col gap-2.5 rounded-lg border bg-secondary px-4 py-3.5';

/** Bar width as a share of the tile's largest count; never invisible. */
function barWidth(count: number, max: number): string {
  if (max <= 0 || count <= 0) return '0%';
  return `${Math.max((count / max) * 100, 3)}%`;
}

function MiniBarTile({
  tile,
  className,
}: {
  tile: StatTile;
  className?: string;
}) {
  const { t } = useTranslation('common');
  const max = Math.max(0, ...tile.rows.map((row) => row.count));
  const shown = tile.rows.slice(0, MAX_TILE_ROWS);
  const hidden = tile.rows.slice(MAX_TILE_ROWS);

  return (
    <div className={cn(TILE_CLASS, className)}>
      <span className="text-[12.5px] font-medium text-low">{tile.label}</span>
      {tile.rows.length === 0 ? (
        <span className="text-xs text-low">
          {t('kanban.knowledge.notTagged', 'Not tagged yet')}
        </span>
      ) : (
        <div className="flex flex-col gap-1.5">
          {shown.map((row) => (
            <div
              key={row.key}
              title={`${row.label}: ${row.count}`}
              className="grid grid-cols-[76px_minmax(0,1fr)_minmax(28px,auto)] items-center gap-2 text-xs text-normal"
            >
              <span className="truncate">{row.label}</span>
              <span className="h-1.5 overflow-hidden rounded-r-[3px] bg-panel">
                <span
                  className="block h-full rounded-r-[3px] bg-brand"
                  style={{ width: barWidth(row.count, max) }}
                />
              </span>
              <span className="text-right font-mono">{row.count}</span>
            </div>
          ))}
          {hidden.length > 0 && (
            <span
              className="text-xs text-low"
              title={hidden
                .map((row) => `${row.label}: ${row.count}`)
                .join(', ')}
            >
              {t('kanban.knowledge.moreValues', '+{{count}} more', {
                count: hidden.length,
              })}
            </span>
          )}
        </div>
      )}
    </div>
  );
}

interface KnowledgeTotalsProps {
  total: number;
  /** Small muted line under the hero number; omitted when null. */
  footnote: string | null;
  /** Always shown, in order. */
  primaryTiles: StatTile[];
  /** Shown on a second row, under the primary tiles, only when non-empty. */
  secondaryTiles: StatTile[];
}

/** Hero RCA count plus one mini-bar tile per tag namespace. */
export function KnowledgeTotals({
  total,
  footnote,
  primaryTiles,
  secondaryTiles,
}: KnowledgeTotalsProps) {
  const { t } = useTranslation('common');

  return (
    <section
      aria-label={t('kanban.knowledge.rcaCount', 'RCAs stored')}
      className="grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-[200px_repeat(4,minmax(0,1fr))]"
    >
      <div
        className={cn(TILE_CLASS, 'col-span-2 justify-between md:col-span-1')}
      >
        <span className="text-[12.5px] font-medium text-low">
          {t('kanban.knowledge.rcaCount', 'RCAs stored')}
        </span>
        <span className="text-[52px] font-semibold leading-none tracking-[-1px] text-high">
          {total}
        </span>
        {footnote && <span className="text-[12.5px] text-low">{footnote}</span>}
      </div>
      {primaryTiles.map((tile) => (
        <MiniBarTile key={tile.id} tile={tile} />
      ))}
      {secondaryTiles.map((tile, index) => (
        <MiniBarTile
          key={tile.id}
          tile={tile}
          className={index === 0 ? 'xl:col-start-2' : undefined}
        />
      ))}
    </section>
  );
}
