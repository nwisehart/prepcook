import {
  ArrowUUpLeftIcon,
  GearIcon,
  RobotIcon,
  UserIcon,
} from '@phosphor-icons/react';
import { Fragment, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { cn } from '../lib/cn';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
} from './Dropdown';

export interface AppBarAgentAccount {
  id: string;
  name: string;
}

interface AppBarUserPopoverProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSettings?: () => void;
  /** Agent accounts an admin can act as (act-as-agent switcher). */
  agentAccounts?: AppBarAgentAccount[];
  onActAsAgent?: (agentId: string) => void;
  /** True while the active session is an impersonated agent. */
  actingAsAgent?: boolean;
  onSwitchBack?: () => void;
}

export function AppBarUserPopover({
  open,
  onOpenChange,
  onSettings,
  agentAccounts = [],
  onActAsAgent,
  actingAsAgent = false,
  onSwitchBack,
}: AppBarUserPopoverProps) {
  const { t } = useTranslation();
  const settingsLabel = t('settings:settings.layout.nav.title', {
    defaultValue: 'Settings',
  });

  // Menu groups in display order; a separator is drawn between each pair so
  // the menu never starts or ends with one whichever groups are present.
  const menuGroups: ReactNode[] = [];
  if (actingAsAgent && onSwitchBack) {
    menuGroups.push(
      <DropdownMenuItem
        key="switch-back"
        icon={ArrowUUpLeftIcon}
        onClick={onSwitchBack}
      >
        {t('agentSwitcher.switchBack', {
          defaultValue: 'Switch back to my account',
        })}
      </DropdownMenuItem>
    );
  }
  if (!actingAsAgent && onActAsAgent && agentAccounts.length > 0) {
    menuGroups.push(
      <Fragment key="act-as-agent">
        <DropdownMenuLabel>
          {t('agentSwitcher.actAsAgent', { defaultValue: 'Act as agent' })}
        </DropdownMenuLabel>
        {agentAccounts.map((agent) => (
          <DropdownMenuItem
            key={agent.id}
            icon={RobotIcon}
            onClick={() => onActAsAgent(agent.id)}
          >
            <span className="truncate">{agent.name}</span>
          </DropdownMenuItem>
        ))}
      </Fragment>
    );
  }
  if (onSettings) {
    menuGroups.push(
      <DropdownMenuItem key="settings" icon={GearIcon} onClick={onSettings}>
        {settingsLabel}
      </DropdownMenuItem>
    );
  }

  return (
    <DropdownMenu open={open} onOpenChange={onOpenChange}>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          className={cn(
            'flex size-8 shrink-0 items-center justify-center rounded',
            'text-low transition-colors cursor-pointer hover:bg-panel hover:text-high',
            'data-[state=open]:bg-panel data-[state=open]:text-high',
            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
          )}
          aria-label={t('appBar.account')}
        >
          <UserIcon className="size-4" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent side="bottom" align="end" className="min-w-[200px]">
        {menuGroups.flatMap((group, index) =>
          index === 0
            ? [group]
            : [<DropdownMenuSeparator key={`separator-${index}`} />, group]
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
