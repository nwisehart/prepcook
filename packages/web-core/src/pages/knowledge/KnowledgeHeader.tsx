import { useTranslation } from 'react-i18next';
import {
  CalendarBlankIcon,
  CaretDownIcon,
  DownloadSimpleIcon,
} from '@phosphor-icons/react';
import { Button } from '@prepcook/ui/components/Button';
import { Input } from '@prepcook/ui/components/Input';
import {
  ButtonGroup,
  ButtonGroupItem,
} from '@prepcook/ui/components/IconButtonGroup';
import {
  Popover,
  PopoverClose,
  PopoverContent,
  PopoverTrigger,
} from '@prepcook/ui/components/Popover';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from '@prepcook/ui/components/Dropdown';
import type { DayRange } from './knowledgeStats';

export type KnowledgeViewMode = 'ranking' | 'web';
export type ExportFormat = 'json' | 'csv';

const EXPORT_FORMAT_LABELS: Record<ExportFormat, string> = {
  json: 'JSON',
  csv: 'CSV',
};

interface KnowledgeHeaderProps {
  range: DayRange;
  /** The range as the button shows it ("All time", "Oct 1 – Oct 18"). */
  rangeLabel: string;
  onRangeChange: (range: DayRange) => void;
  view: KnowledgeViewMode;
  onViewChange: (view: KnowledgeViewMode) => void;
  exportFormat: ExportFormat;
  onExportFormatChange: (format: ExportFormat) => void;
  onExport: () => void;
}

export function KnowledgeHeader({
  range,
  rangeLabel,
  onRangeChange,
  view,
  onViewChange,
  exportFormat,
  onExportFormatChange,
  onExport,
}: KnowledgeHeaderProps) {
  const { t } = useTranslation('common');
  const hasRange = Boolean(range.since || range.until);

  return (
    <header className="flex flex-wrap items-center gap-3">
      <div className="mr-auto flex min-w-0 flex-col gap-0.5">
        <h1 className="text-[22px] font-bold leading-tight tracking-[-0.3px] text-high">
          {t('kanban.knowledge.heading', 'Knowledge')}
        </h1>
        <p className="text-[12.5px] text-low">
          {t(
            'kanban.knowledge.subtitle',
            'Every approved bug RCA, linked by product area, component and root-cause class. A cause that keeps coming back shows up here first.'
          )}
        </p>
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <Popover>
          <PopoverTrigger asChild>
            <Button variant="secondary" size="lg" className="gap-2 px-3">
              <CalendarBlankIcon className="size-4 text-low" />
              {rangeLabel}
              <CaretDownIcon className="size-3.5 text-low" />
            </Button>
          </PopoverTrigger>
          <PopoverContent
            align="end"
            className="flex w-72 flex-col gap-3 px-3.5 py-3.5"
          >
            <div className="flex flex-col gap-0.5">
              <span className="text-sm font-semibold text-high">
                {t('kanban.knowledge.range.title', 'Date range')}
              </span>
              <span className="text-xs text-low">
                {t(
                  'kanban.knowledge.range.help',
                  'Shows and exports the RCAs filed on these days (UTC). Leave a side empty to leave it open.'
                )}
              </span>
            </div>
            <label className="flex flex-col gap-1 text-xs font-medium text-low">
              {t('kanban.knowledge.range.from', 'From')}
              <Input
                type="date"
                value={range.since}
                max={range.until || undefined}
                onChange={(event) =>
                  onRangeChange({ ...range, since: event.target.value })
                }
              />
            </label>
            <label className="flex flex-col gap-1 text-xs font-medium text-low">
              {t('kanban.knowledge.range.to', 'To')}
              <Input
                type="date"
                value={range.until}
                min={range.since || undefined}
                onChange={(event) =>
                  onRangeChange({ ...range, until: event.target.value })
                }
              />
            </label>
            <div className="flex items-center justify-between">
              <Button
                variant="ghost"
                size="sm"
                disabled={!hasRange}
                onClick={() => onRangeChange({ since: '', until: '' })}
              >
                {t('kanban.knowledge.range.clear', 'Clear')}
              </Button>
              <PopoverClose asChild>
                <Button variant="secondary" size="sm">
                  {t('buttons.close')}
                </Button>
              </PopoverClose>
            </div>
          </PopoverContent>
        </Popover>

        <ButtonGroup className="h-9">
          <ButtonGroupItem
            active={view === 'ranking'}
            onClick={() => onViewChange('ranking')}
          >
            {t('kanban.knowledge.viewRanking', 'Ranking')}
          </ButtonGroupItem>
          <ButtonGroupItem
            active={view === 'web'}
            onClick={() => onViewChange('web')}
          >
            {t('kanban.knowledge.viewWeb', 'Web')}
          </ButtonGroupItem>
        </ButtonGroup>

        <div className="flex">
          <Button
            variant="secondary"
            size="lg"
            className="gap-2 rounded-r-none px-3"
            onClick={onExport}
          >
            <DownloadSimpleIcon className="size-4 text-low" />
            {t('kanban.knowledge.exportAs', 'Export {{format}}', {
              format: EXPORT_FORMAT_LABELS[exportFormat],
            })}
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="secondary"
                size="lg"
                className="rounded-l-none border-l-0 px-2"
                aria-label={t('kanban.knowledge.exportFormat', 'Export format')}
              >
                <CaretDownIcon className="size-3.5 text-low" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuRadioGroup
                value={exportFormat}
                onValueChange={(value) =>
                  onExportFormatChange(value === 'csv' ? 'csv' : 'json')
                }
              >
                {(Object.keys(EXPORT_FORMAT_LABELS) as ExportFormat[]).map(
                  (format) => (
                    <DropdownMenuRadioItem key={format} value={format}>
                      {EXPORT_FORMAT_LABELS[format]}
                    </DropdownMenuRadioItem>
                  )
                )}
              </DropdownMenuRadioGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>
    </header>
  );
}
