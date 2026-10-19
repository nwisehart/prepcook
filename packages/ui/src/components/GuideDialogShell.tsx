import { useState, type ReactNode } from 'react';
import { CaretLeftIcon, XIcon } from '@phosphor-icons/react';
import { cn } from '../lib/cn';

export interface GuideDialogTopic {
  id: string;
  title: string;
  content: ReactNode;
  imageSrc?: string;
  imageAlt?: string;
}

interface GuideDialogShellProps {
  topics: GuideDialogTopic[];
  closeLabel: string;
  onClose: () => void;
  className?: string;
}

export function GuideDialogShell({
  topics,
  closeLabel,
  onClose,
  className,
}: GuideDialogShellProps) {
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [mobileShowContent, setMobileShowContent] = useState(false);

  if (topics.length === 0) {
    return null;
  }

  const selectedTopic = topics[selectedIndex] ?? topics[0];

  return (
    <>
      <div
        data-tauri-drag-region
        className="fixed inset-0 z-[9998] bg-scrim/45 animate-in fade-in-0 duration-200"
        onClick={onClose}
      />
      {/* Dialog wrapper - handles positioning */}
      <div
        className={cn(
          'fixed z-[9999]',
          // Mobile: full screen
          'inset-0',
          // Desktop: centered with fixed size
          'md:inset-auto md:left-1/2 md:top-1/2 md:-translate-x-1/2 md:-translate-y-1/2'
        )}
      >
        <div
          className={cn(
            'h-full w-full flex overflow-hidden',
            'bg-secondary text-normal',
            'animate-in fade-in-0 slide-in-from-bottom-4 duration-200',
            // Mobile: full screen, no rounded corners
            'rounded-none border-0',
            // Desktop: fixed size with rounded corners
            'md:w-[800px] md:h-[600px] md:rounded-xl md:border md:border-border md:shadow-overlay',
            className
          )}
        >
          {/* Sidebar - hidden on mobile when showing content */}
          <div
            className={cn(
              'bg-secondary border-r border-border flex flex-col',
              // Mobile: full width, hidden when showing content
              'w-full',
              mobileShowContent && 'hidden',
              // Desktop: fixed width sidebar, always visible
              'md:w-52 md:flex'
            )}
          >
            {/* Header with mobile close button */}
            <div className="p-3 flex items-center justify-between md:hidden">
              <span className="text-sm font-semibold text-high">Topics</span>
              <button
                type="button"
                onClick={onClose}
                className="rounded p-1 text-low hover:bg-panel hover:text-high"
              >
                <XIcon className="size-4" />
              </button>
            </div>
            <nav className="flex-1 px-3 py-[18px] flex flex-col gap-0.5 overflow-y-auto">
              {topics.map((topic, idx) => (
                <button
                  key={topic.id}
                  type="button"
                  onClick={() => {
                    setSelectedIndex(idx);
                    setMobileShowContent(true);
                  }}
                  aria-current={idx === selectedIndex ? 'page' : undefined}
                  className={cn(
                    'text-left px-2.5 py-2 rounded text-sm text-high transition-colors',
                    'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
                    idx === selectedIndex
                      ? 'bg-panel font-semibold'
                      : 'font-medium hover:bg-panel'
                  )}
                >
                  {topic.title}
                </button>
              ))}
            </nav>
          </div>
          {/* Content - hidden on mobile when showing nav */}
          <div
            className={cn(
              'flex-1 flex flex-col relative overflow-y-auto',
              // Mobile: full width, hidden when showing nav
              !mobileShowContent && 'hidden',
              // Desktop: always visible
              'md:flex'
            )}
          >
            {/* Mobile header with back button */}
            <div className="flex items-center gap-2 p-3 border-b border-border md:hidden">
              <button
                type="button"
                onClick={() => setMobileShowContent(false)}
                className="rounded p-1 text-low hover:bg-panel hover:text-high"
              >
                <CaretLeftIcon className="size-4" />
              </button>
              <span className="text-sm font-semibold text-high">Back</span>
              <button
                type="button"
                onClick={onClose}
                className="ml-auto rounded p-1 text-low hover:bg-panel hover:text-high"
              >
                <XIcon className="size-4" />
              </button>
            </div>
            {/* Desktop close button */}
            <button
              type="button"
              onClick={onClose}
              className="absolute right-4 top-4 hidden size-8 items-center justify-center rounded text-low transition-colors hover:bg-panel hover:text-high focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand md:flex"
            >
              <XIcon className="size-4" />
              <span className="sr-only">{closeLabel}</span>
            </button>
            <div className="p-6 pt-4 md:pt-[22px] flex-1">
              <h2 className="text-lg font-bold text-high mb-4 pr-10">
                {selectedTopic.title}
              </h2>
              {selectedTopic.imageSrc && (
                <img
                  src={selectedTopic.imageSrc}
                  alt={selectedTopic.imageAlt ?? selectedTopic.title}
                  className="w-full rounded-lg border border-border mb-4"
                />
              )}
              <div className="text-normal text-sm leading-relaxed flex flex-col gap-3">
                {typeof selectedTopic.content === 'string' ? (
                  <p>{selectedTopic.content}</p>
                ) : (
                  selectedTopic.content
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    </>
  );
}
