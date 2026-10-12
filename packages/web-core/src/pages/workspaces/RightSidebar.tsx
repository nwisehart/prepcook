import { memo, useMemo } from 'react';
import { ProcessListContainer } from './ProcessListContainer';
import { PreviewControlsContainer } from './PreviewControlsContainer';
import { GitPanelContainer } from './GitPanelContainer';
import { TerminalPanelContainer } from '@/shared/components/TerminalPanelContainer';
import { ArrowsOutSimpleIcon } from '@phosphor-icons/react';
import { useLogsPanel } from '@/shared/hooks/useLogsPanel';
import type { RepoWithTargetBranch, Workspace } from 'shared/types';
import {
  PERSIST_KEYS,
  PersistKey,
  RIGHT_MAIN_PANEL_MODES,
  type RightMainPanelMode,
  usePersistedExpanded,
  useUiPreferencesStore,
} from '@/shared/stores/useUiPreferencesStore';
import {
  CollapsibleSectionHeader,
  type SectionAction,
} from '@prepcook/ui/components/CollapsibleSectionHeader';

type SectionDef = {
  title: string;
  persistKey: PersistKey;
  visible: boolean;
  expanded: boolean;
  content: React.ReactNode;
  actions: SectionAction[];
};

export interface RightSidebarProps {
  rightMainPanelMode: RightMainPanelMode | null;
  selectedWorkspace: Workspace | undefined;
  repos: RepoWithTargetBranch[];
}

export const RightSidebar = memo(function RightSidebar({
  rightMainPanelMode,
  selectedWorkspace,
  repos,
}: RightSidebarProps) {
  const isTerminalVisible = useUiPreferencesStore((s) => s.isTerminalVisible);
  const { expandTerminal, isTerminalExpanded } = useLogsPanel();

  const [processesExpanded] = usePersistedExpanded(
    PERSIST_KEYS.processesSection,
    true
  );
  const [devServerExpanded] = usePersistedExpanded(
    PERSIST_KEYS.devServerSection,
    true
  );
  const [gitExpanded] = usePersistedExpanded(
    PERSIST_KEYS.gitPanelRepositories,
    true
  );
  const [terminalExpanded] = usePersistedExpanded(
    PERSIST_KEYS.terminalSection,
    false
  );

  // The Review tab carries its own file-tree navigator, so only Logs and
  // Preview add a companion section here.
  const hasUpperContent =
    rightMainPanelMode === RIGHT_MAIN_PANEL_MODES.LOGS ||
    rightMainPanelMode === RIGHT_MAIN_PANEL_MODES.PREVIEW;

  const upperExpanded = (() => {
    if (rightMainPanelMode === RIGHT_MAIN_PANEL_MODES.LOGS)
      return processesExpanded;
    if (rightMainPanelMode === RIGHT_MAIN_PANEL_MODES.PREVIEW)
      return devServerExpanded;
    return false;
  })();

  const sections: SectionDef[] = useMemo(() => {
    const result: SectionDef[] = [
      {
        title: 'Git',
        persistKey: PERSIST_KEYS.gitPanelRepositories,
        visible: true,
        expanded: gitExpanded,
        content: (
          <GitPanelContainer
            selectedWorkspace={selectedWorkspace}
            repos={repos}
          />
        ),
        actions: [],
      },
      {
        title: 'Terminal',
        persistKey: PERSIST_KEYS.terminalSection,
        visible: isTerminalVisible && !isTerminalExpanded,
        expanded: terminalExpanded,
        content: <TerminalPanelContainer />,
        actions: [{ icon: ArrowsOutSimpleIcon, onClick: expandTerminal }],
      },
    ];

    switch (rightMainPanelMode) {
      case RIGHT_MAIN_PANEL_MODES.LOGS:
        result.unshift({
          title: 'Logs',
          persistKey: PERSIST_KEYS.rightPanelprocesses,
          visible: hasUpperContent,
          expanded: upperExpanded,
          content: <ProcessListContainer />,
          actions: [],
        });
        break;
      case RIGHT_MAIN_PANEL_MODES.PREVIEW:
        if (selectedWorkspace) {
          result.unshift({
            title: 'Preview',
            persistKey: PERSIST_KEYS.rightPanelPreview,
            visible: hasUpperContent,
            expanded: upperExpanded,
            content: (
              <PreviewControlsContainer
                workspaceId={selectedWorkspace.id}
                className=""
              />
            ),
            actions: [],
          });
        }
        break;
      case RIGHT_MAIN_PANEL_MODES.REVIEW:
      case null:
        break;
    }

    return result;
  }, [
    rightMainPanelMode,
    selectedWorkspace,
    repos,
    gitExpanded,
    terminalExpanded,
    processesExpanded,
    devServerExpanded,
    isTerminalVisible,
    isTerminalExpanded,
    hasUpperContent,
    upperExpanded,
    expandTerminal,
  ]);

  return (
    <div className="h-full border-l bg-secondary overflow-y-auto">
      <div className="divide-y border-b">
        {sections
          .filter((section) => section.visible)
          .map((section) => (
            <div
              key={section.persistKey}
              className="max-h-[max(50vh,400px)] flex flex-col overflow-hidden"
            >
              <CollapsibleSectionHeader
                title={section.title}
                persistKey={section.persistKey}
                defaultExpanded={section.expanded}
                actions={section.actions}
              >
                <div className="flex flex-1 border-t min-h-[200px] w-full overflow-auto">
                  {section.content}
                </div>
              </CollapsibleSectionHeader>
            </div>
          ))}
      </div>
    </div>
  );
});
