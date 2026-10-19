import { useEffect, useState, type ReactNode } from 'react';
import { CaretDownIcon } from '@phosphor-icons/react';
import { cn } from '../lib/cn';
import type { SectionAction } from './CollapsibleSectionHeader';

// Shared with CollapsibleSectionHeader so a section keeps the open/closed
// state a person already chose.
const STORAGE_KEY_PREFIX = 'vibe.ui.collapsible.';

function readExpanded(persistKey: string | undefined, fallback: boolean) {
  if (!persistKey || typeof window === 'undefined') return fallback;
  try {
    const stored = window.localStorage.getItem(
      `${STORAGE_KEY_PREFIX}${persistKey}`
    );
    return stored == null ? fallback : stored === 'true';
  } catch {
    return fallback;
  }
}

export interface IssuePanelSectionProps {
  /** Shown as an 11px uppercase label. */
  label: string;
  /** Icon actions on the right of the label (add, link, …). */
  actions?: SectionAction[];
  /** Any other control on the right of the label. */
  headerExtra?: ReactNode;
  /** Makes the label a toggle; the state persists under this key. */
  persistKey?: string;
  collapsible?: boolean;
  defaultExpanded?: boolean;
  /** The hairline above; off for the first section under the header. */
  divider?: boolean;
  children?: ReactNode;
  className?: string;
}

/**
 * A section of the card panel: a hairline above, an uppercase label with
 * its actions, then the content.
 */
export function IssuePanelSection({
  label,
  actions = [],
  headerExtra,
  persistKey,
  collapsible = false,
  defaultExpanded = true,
  divider = true,
  children,
  className,
}: IssuePanelSectionProps) {
  const [expanded, setExpanded] = useState(() =>
    readExpanded(persistKey, defaultExpanded)
  );

  useEffect(() => {
    setExpanded(readExpanded(persistKey, defaultExpanded));
  }, [persistKey, defaultExpanded]);

  useEffect(() => {
    if (!persistKey || !collapsible) return;
    try {
      window.localStorage.setItem(
        `${STORAGE_KEY_PREFIX}${persistKey}`,
        String(expanded)
      );
    } catch {
      // Storage may be unavailable (private mode, quota); the state just
      // does not persist.
    }
  }, [persistKey, collapsible, expanded]);

  const isExpanded = collapsible ? expanded : true;
  const labelClass =
    'text-[11px] font-bold uppercase tracking-[0.08em] text-low';

  return (
    <section
      className={cn(
        'flex flex-col gap-2 py-3.5',
        divider && 'border-t border-divider',
        className
      )}
    >
      <div className="flex min-h-6 items-center justify-between gap-2">
        {collapsible ? (
          <button
            type="button"
            onClick={() => setExpanded((value) => !value)}
            aria-expanded={isExpanded}
            className={cn(
              labelClass,
              'flex items-center gap-1 rounded-sm hover:text-normal focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
            )}
          >
            {label}
            <CaretDownIcon
              weight="bold"
              className={cn(
                'size-3 transition-transform',
                !isExpanded && '-rotate-90'
              )}
            />
          </button>
        ) : (
          <span className={labelClass}>{label}</span>
        )}
        {(headerExtra || actions.length > 0) && (
          <div className="flex items-center gap-1">
            {headerExtra}
            {actions.map((action, index) => {
              const ActionIcon = action.icon;
              return (
                <button
                  key={index}
                  type="button"
                  onClick={action.onClick}
                  className={cn(
                    'flex size-6 items-center justify-center rounded-sm transition-colors hover:bg-panel hover:text-high',
                    action.isActive ? 'text-brand' : 'text-low'
                  )}
                >
                  <ActionIcon className="size-3.5" weight="bold" />
                </button>
              );
            })}
          </div>
        )}
      </div>
      {isExpanded && children}
    </section>
  );
}
