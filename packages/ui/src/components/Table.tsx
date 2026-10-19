import * as React from 'react';
import { cn } from '../lib/cn';

const Table = React.forwardRef<
  HTMLTableElement,
  React.HTMLAttributes<HTMLTableElement>
>(({ className, ...props }, ref) => (
  <table
    ref={ref}
    className={cn('w-full text-sm text-normal', className)}
    {...props}
  />
));
Table.displayName = 'Table';

const TableHead = React.forwardRef<
  HTMLTableSectionElement,
  React.HTMLAttributes<HTMLTableSectionElement>
>(({ className, ...props }, ref) => (
  <thead
    ref={ref}
    className={cn(
      'text-xs font-bold uppercase tracking-[0.08em] text-low',
      className
    )}
    {...props}
  />
));
TableHead.displayName = 'TableHead';

const TableBody = React.forwardRef<
  HTMLTableSectionElement,
  React.HTMLAttributes<HTMLTableSectionElement>
>(({ className, ...props }, ref) => (
  <tbody ref={ref} className={className} {...props} />
));
TableBody.displayName = 'TableBody';

const TableRow = React.forwardRef<
  HTMLTableRowElement,
  React.HTMLAttributes<HTMLTableRowElement> & {
    clickable?: boolean;
  }
>(({ className, clickable, ...props }, ref) => (
  <tr
    ref={ref}
    className={cn(
      'border-t border-divider',
      clickable && 'cursor-pointer transition-colors hover:bg-panel',
      className
    )}
    {...props}
  />
));
TableRow.displayName = 'TableRow';

const TableHeaderCell = React.forwardRef<
  HTMLTableCellElement,
  React.ThHTMLAttributes<HTMLTableCellElement>
>(({ className, ...props }, ref) => (
  <th ref={ref} className={cn('py-2 text-left', className)} {...props} />
));
TableHeaderCell.displayName = 'TableHeaderCell';

const TableCell = React.forwardRef<
  HTMLTableCellElement,
  React.TdHTMLAttributes<HTMLTableCellElement>
>(({ className, ...props }, ref) => (
  <td ref={ref} className={cn('py-2', className)} {...props} />
));
TableCell.displayName = 'TableCell';

const TableEmpty = ({
  colSpan,
  children,
}: {
  colSpan: number;
  children: React.ReactNode;
}) => (
  <TableRow>
    <TableCell colSpan={colSpan} className="text-low">
      {children}
    </TableCell>
  </TableRow>
);

const TableLoading = ({ colSpan }: { colSpan: number }) => (
  <TableRow>
    <TableCell colSpan={colSpan}>
      <div className="h-5 w-full bg-panel rounded animate-pulse" />
    </TableCell>
  </TableRow>
);

export {
  Table,
  TableHead,
  TableBody,
  TableRow,
  TableHeaderCell,
  TableCell,
  TableEmpty,
  TableLoading,
};
