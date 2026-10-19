import { type ReactNode } from 'react';
import { createPortal } from 'react-dom';

import { cn } from '../lib/cn';

interface MobileDrawerProps {
  open: boolean;
  onClose: () => void;
  children: ReactNode;
}

export function MobileDrawer({ open, onClose, children }: MobileDrawerProps) {
  return createPortal(
    <>
      {/* Backdrop overlay */}
      <div
        data-tauri-drag-region
        className={cn(
          'fixed inset-0 z-[100] bg-scrim/45',
          'transition-opacity duration-200 ease-out',
          open ? 'opacity-100' : 'pointer-events-none opacity-0'
        )}
        onClick={onClose}
        aria-hidden="true"
      />
      {/* Drawer panel */}
      <div
        className={cn(
          'fixed left-0 top-0 z-[101] h-full w-[280px] bg-primary shadow-overlay',
          'pb-[env(safe-area-inset-bottom)]',
          'transition-transform duration-200 ease-out',
          open ? 'translate-x-0' : 'pointer-events-none -translate-x-full'
        )}
      >
        {children}
      </div>
    </>,
    document.body
  );
}
