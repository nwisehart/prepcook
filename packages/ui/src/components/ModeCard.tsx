import { useTranslation } from 'react-i18next';
import { cn } from '../lib/cn';
import { Switch } from './Switch';

/** Chef mode: autonomy off. Prep mode: autonomy on. */
export type OrchestratorModeName = 'chef' | 'prep';

interface ModeCardProps {
  mode: OrchestratorModeName;
  /**
   * Called by the switch and the "Switch to …" link. The card never changes
   * the mode itself; the caller decides where the change happens.
   */
  onSwitchMode?: () => void;
  className?: string;
}

/**
 * Sidebar card naming the orchestrator's current mode, with a switch that
 * mirrors it and a one-line description of what the mode means.
 */
export function ModeCard({ mode, onSwitchMode, className }: ModeCardProps) {
  const { t } = useTranslation('common');
  const isPrepMode = mode === 'prep';
  const title = isPrepMode
    ? t('appBar.mode.prep.title')
    : t('appBar.mode.chef.title');
  const description = isPrepMode
    ? t('appBar.mode.prep.description')
    : t('appBar.mode.chef.description');
  const switchLabel = isPrepMode
    ? t('appBar.mode.prep.switch')
    : t('appBar.mode.chef.switch');

  return (
    <section
      aria-label={t('appBar.mode.label')}
      className={cn(
        'flex flex-col gap-2 rounded-lg border bg-secondary p-3',
        className
      )}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="text-sm font-semibold text-high">{title}</span>
        <Switch
          checked={isPrepMode}
          onCheckedChange={() => onSwitchMode?.()}
          disabled={!onSwitchMode}
          aria-label={switchLabel}
        />
      </div>
      <p className="text-xs text-low">{description}</p>
      {onSwitchMode && (
        <button
          type="button"
          onClick={onSwitchMode}
          className={cn(
            'self-start rounded-sm text-xs font-semibold text-brand-secondary',
            'cursor-pointer hover:underline underline-offset-2',
            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand'
          )}
        >
          {switchLabel}
        </button>
      )}
    </section>
  );
}
