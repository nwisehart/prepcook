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
            'flex items-center justify-center w-7 h-7 sm:w-10 sm:h-10 rounded-md sm:rounded-lg',
            'transition-colors cursor-pointer overflow-hidden',
            'focus:outline-none focus-visible:ring-2 focus-visible:ring-brand',
            'bg-panel text-normal font-medium text-sm hover:bg-panel/70'
          )}
          aria-label="Account"
        >
          <UserIcon className="size-icon-sm" weight="bold" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent side="right" align="end" className="min-w-[200px]">
        {menuGroups.flatMap((group, index) =>
          index === 0
            ? [group]
            : [<DropdownMenuSeparator key={`separator-${index}`} />, group]
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
