import { useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { CopyIcon, DotsThreeIcon } from '@phosphor-icons/react';
import { Button } from '@prepcook/ui/components/Button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@prepcook/ui/components/Dropdown';
import { useActions } from '@/shared/hooks/useActions';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { IdeIcon } from '@/shared/components/IdeIcon';
import { Actions, ContextBarActionGroups } from '@/shared/actions';
import {
  type ActionDefinition,
  type ActionVisibilityContext,
  ActionTargetType,
  getActionIcon,
  getActionLabel,
  getActionTooltip,
  isActionEnabled,
  isActionVisible,
  isSpecialIcon,
} from '@/shared/types/actions';
import type { EditorType } from 'shared/types';
import { useActionVisibilityContext } from '@/shared/hooks/useActionVisibilityContext';
import { isRealMobileDevice } from '@/shared/hooks/useIsMobile';
import { cn } from '@/shared/lib/utils';

/** Actions surfaced as their own header button rather than in the menu. */
const HEADER_BUTTON_ACTIONS = new Set<string>([Actions.OpenSpecReview.id]);

function visibleMenuActions(
  actions: readonly ActionDefinition[],
  ctx: ActionVisibilityContext
): ActionDefinition[] {
  return actions.filter(
    (action) =>
      !HEADER_BUTTON_ACTIONS.has(action.id) &&
      isActionVisible(action, ctx) &&
      // Opening a desktop IDE means nothing on a phone or tablet.
      !(action.icon === 'ide-icon' && isRealMobileDevice())
  );
}

/** The menu wording: the tooltip where it names the target, else the label. */
function menuLabel(
  action: ActionDefinition,
  ctx: ActionVisibilityContext
): string {
  return action.getTooltip
    ? getActionTooltip(action, ctx)
    : getActionLabel(action, ctx);
}

function MenuActionIcon({
  action,
  ctx,
  editorType,
}: {
  action: ActionDefinition;
  ctx: ActionVisibilityContext;
  editorType: EditorType | null;
}) {
  const icon = getActionIcon(action, ctx);
  if (icon === 'ide-icon') {
    return <IdeIcon editorType={editorType} className="size-icon-xs" />;
  }
  if (isSpecialIcon(icon)) {
    return <CopyIcon className="size-icon-xs" />;
  }
  const Icon = icon;
  const isDevServerBusy =
    action.id === Actions.ToggleDevServer.id &&
    (ctx.devServerState === 'starting' || ctx.devServerState === 'stopping');
  return (
    <Icon className={cn('size-icon-xs', isDevServerBusy && 'animate-spin')} />
  );
}

/**
 * The session header's buttons: "Review plan" while a plan review session is
 * open for this workspace, and a "More" menu holding the workspace actions
 * (open in IDE, copy path, dev server, preview and review panels).
 */
export function SessionHeaderActionsContainer() {
  const { t } = useTranslation('common');
  const { executeAction } = useActions();
  const { config } = useUserSystem();
  const editorType =
    (config?.editor?.editor_type as EditorType | undefined) ?? null;
  const actionCtx = useActionVisibilityContext();

  const handleExecuteAction = useCallback(
    (action: ActionDefinition) => {
      if (action.requiresTarget === ActionTargetType.NONE) {
        void executeAction(action);
      }
    },
    [executeAction]
  );

  const menuGroups = useMemo(
    () =>
      [
        visibleMenuActions(ContextBarActionGroups.primary, actionCtx),
        visibleMenuActions(ContextBarActionGroups.secondary, actionCtx),
      ].filter((group) => group.length > 0),
    [actionCtx]
  );

  const showReviewPlan = isActionVisible(Actions.OpenSpecReview, actionCtx);

  if (isRealMobileDevice() && !showReviewPlan) return null;

  return (
    <>
      {showReviewPlan && (
        <Button
          variant="secondary"
          onClick={() => handleExecuteAction(Actions.OpenSpecReview)}
          title={getActionTooltip(Actions.OpenSpecReview, actionCtx)}
        >
          {t('workspaces.header.reviewPlan')}
        </Button>
      )}
      {!isRealMobileDevice() && menuGroups.length > 0 && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="secondary"
              size="icon"
              aria-label={t('workspaces.more')}
              title={t('workspaces.more')}
            >
              <DotsThreeIcon className="size-4" weight="bold" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {menuGroups.map((group, groupIndex) => (
              <div key={groupIndex}>
                {groupIndex > 0 && <DropdownMenuSeparator />}
                {group.map((action) => (
                  <DropdownMenuItem
                    key={action.id}
                    disabled={!isActionEnabled(action, actionCtx)}
                    onSelect={() => handleExecuteAction(action)}
                  >
                    <MenuActionIcon
                      action={action}
                      ctx={actionCtx}
                      editorType={editorType}
                    />
                    <span>{menuLabel(action, actionCtx)}</span>
                    {action.shortcut && (
                      <span className="ml-auto pl-4 font-mono text-xs text-low">
                        {action.shortcut}
                      </span>
                    )}
                  </DropdownMenuItem>
                ))}
              </div>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </>
  );
}
