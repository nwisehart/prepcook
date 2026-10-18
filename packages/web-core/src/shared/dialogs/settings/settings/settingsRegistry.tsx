import {
  GearIcon,
  GitBranchIcon,
  BookOpenIcon,
  CloudIcon,
  CpuIcon,
  PlugIcon,
  RobotIcon,
  UsersIcon,
  ChartBarIcon,
  SparkleIcon,
  PlugsConnectedIcon,
  BinocularsIcon,
  TicketIcon,
  TreeIcon,
  KeyIcon,
  KanbanIcon,
  BellIcon,
  BugIcon,
  UserCheckIcon,
} from '@phosphor-icons/react';
import type { Icon } from '@phosphor-icons/react';
import { GeneralSettingsSection } from './GeneralSettingsSection';
import { ReposSettingsSection } from './ReposSettingsSection';
import { SecretsSettingsSection } from './SecretsSettingsSection';
import { BoardSettingsSection } from './BoardSettingsSection';
import { OrganizationsSettingsSection } from './OrganizationsSettingsSection';
import { RemoteProjectsSettingsSection } from './RemoteProjectsSettingsSection';
import { AgentsSettingsSection } from './AgentsSettingsSection';
import { McpSettingsSection } from './McpSettingsSection';
import { TicketingSettingsSection } from './TicketingSettingsSection';
import { PlaybooksSettingsSection } from './PlaybooksSettingsSection';
import { WorktreePoolSettingsSection } from './WorktreePoolSettingsSection';
import { OrchestratorSettingsSection } from './OrchestratorSettingsSection';
import { ClaudeSettingsSection } from './ClaudeSettingsSection';
import { UsageSettingsSection } from './UsageSettingsSection';
import { ConnectorsSettingsSection } from './ConnectorsSettingsSection';
import { IntegrationsSettingsSection } from './IntegrationsSettingsSection';
import { NotificationsSettingsSection } from './NotificationsSettingsSection';
import { ReproductionSettingsSection } from './ReproductionSettingsSection';
import { EvidenceSettingsSection } from './EvidenceSettingsSection';

export type SettingsSectionType =
  | 'general'
  | 'repos'
  | 'secrets'
  | 'board'
  | 'organizations'
  | 'remote-projects'
  | 'agents'
  | 'mcp'
  | 'ticketing'
  | 'playbooks'
  | 'orchestrator'
  | 'integrations'
  | 'claude'
  | 'usage'
  | 'connectors'
  | 'worktree-pool'
  | 'notifications'
  | 'reproduction'
  | 'evidence';

/// `integrations` holds one section per integration area, each rendered from
/// its providers' descriptors.
export type SettingsSectionGroup = 'host' | 'integrations' | 'universal';

export type SettingsSectionInitialState = {
  general: undefined;
  repos: { repoId?: string } | undefined;
  secrets: undefined;
  board: undefined;
  organizations: { organizationId?: string } | undefined;
  'remote-projects':
    | { organizationId?: string; projectId?: string }
    | undefined;
  agents: { executor?: string; variant?: string } | undefined;
  mcp: undefined;
  ticketing: undefined;
  playbooks: undefined;
  orchestrator: undefined;
  integrations: undefined;
  claude: undefined;
  usage: undefined;
  connectors: undefined;
  'worktree-pool': undefined;
  notifications: undefined;
  reproduction: undefined;
  evidence: undefined;
};

export interface SettingsSectionDefinition {
  id: SettingsSectionType;
  icon: Icon;
  group: SettingsSectionGroup;
}

export const SETTINGS_SECTION_DEFINITIONS: SettingsSectionDefinition[] = [
  { id: 'general', icon: GearIcon, group: 'host' },
  { id: 'repos', icon: GitBranchIcon, group: 'host' },
  { id: 'secrets', icon: KeyIcon, group: 'host' },
  { id: 'agents', icon: CpuIcon, group: 'host' },
  { id: 'mcp', icon: PlugIcon, group: 'host' },
  { id: 'playbooks', icon: BookOpenIcon, group: 'host' },
  { id: 'orchestrator', icon: RobotIcon, group: 'host' },
  { id: 'integrations', icon: UserCheckIcon, group: 'host' },
  { id: 'claude', icon: SparkleIcon, group: 'host' },
  { id: 'usage', icon: ChartBarIcon, group: 'host' },
  { id: 'connectors', icon: PlugsConnectedIcon, group: 'host' },
  { id: 'worktree-pool', icon: TreeIcon, group: 'host' },
  { id: 'ticketing', icon: TicketIcon, group: 'integrations' },
  { id: 'notifications', icon: BellIcon, group: 'integrations' },
  { id: 'reproduction', icon: BugIcon, group: 'integrations' },
  { id: 'evidence', icon: BinocularsIcon, group: 'integrations' },
  { id: 'board', icon: KanbanIcon, group: 'universal' },
  { id: 'organizations', icon: UsersIcon, group: 'universal' },
  { id: 'remote-projects', icon: CloudIcon, group: 'universal' },
];

export function isHostSpecificSettingsSection(
  type: SettingsSectionType
): boolean {
  // Integration areas are configured per machine like the host sections.
  const group = SETTINGS_SECTION_DEFINITIONS.find(
    (section) => section.id === type
  )?.group;
  return group === 'host' || group === 'integrations';
}

export function renderSettingsSection(
  type: SettingsSectionType,
  initialState?: SettingsSectionInitialState[SettingsSectionType]
) {
  switch (type) {
    case 'general':
      return <GeneralSettingsSection />;
    case 'repos':
      return (
        <ReposSettingsSection
          initialState={initialState as SettingsSectionInitialState['repos']}
        />
      );
    case 'secrets':
      return <SecretsSettingsSection />;
    case 'board':
      return <BoardSettingsSection />;
    case 'organizations':
      return <OrganizationsSettingsSection />;
    case 'remote-projects':
      return (
        <RemoteProjectsSettingsSection
          initialState={
            initialState as SettingsSectionInitialState['remote-projects']
          }
        />
      );
    case 'agents':
      return <AgentsSettingsSection />;
    case 'mcp':
      return <McpSettingsSection />;
    case 'ticketing':
      return <TicketingSettingsSection />;
    case 'playbooks':
      return <PlaybooksSettingsSection />;
    case 'orchestrator':
      return <OrchestratorSettingsSection />;
    case 'integrations':
      return <IntegrationsSettingsSection />;
    case 'claude':
      return <ClaudeSettingsSection />;
    case 'usage':
      return <UsageSettingsSection />;
    case 'connectors':
      return <ConnectorsSettingsSection />;
    case 'worktree-pool':
      return <WorktreePoolSettingsSection />;
    case 'notifications':
      return <NotificationsSettingsSection />;
    case 'reproduction':
      return <ReproductionSettingsSection />;
    case 'evidence':
      return <EvidenceSettingsSection />;
    default:
      return <GeneralSettingsSection />;
  }
}
