import { useTranslation } from 'react-i18next';
import { SpinnerIcon } from '@phosphor-icons/react';
import { useUserOrganizations } from '@/shared/hooks/useUserOrganizations';
import { useSelectedOrgId } from '@/shared/stores/useOrganizationStore';
import { useAuth } from '@/shared/hooks/auth/useAuth';
import { AgentUsersCard } from '@/shared/components/org/AgentUsersCard';
import { MemberRole as MemberRoleEnum } from 'shared/types';

/**
 * The board has one organization and every signed-in user is an admin of
 * it, so the only thing left to manage here is its agent users.
 */
export function OrganizationsSettingsSection() {
  const { t } = useTranslation('organization');
  const { hasBoardAccess, isLoaded } = useAuth();
  const {
    data: orgsResponse,
    isLoading: orgsLoading,
    error: orgsError,
  } = useUserOrganizations();
  const selectedOrgId = useSelectedOrgId();

  const organizations = orgsResponse?.organizations ?? [];
  const selectedOrg =
    organizations.find((org) => org.id === selectedOrgId) ??
    organizations[0] ??
    null;
  const isAdmin = selectedOrg?.user_role === MemberRoleEnum.ADMIN;

  if (!isLoaded || orgsLoading) {
    return (
      <div className="flex items-center justify-center py-8 gap-2">
        <SpinnerIcon
          className="size-icon-lg animate-spin text-brand"
          weight="bold"
        />
        <span className="text-normal">
          {t('settings.loadingOrganizations')}
        </span>
      </div>
    );
  }

  if (!hasBoardAccess) {
    return (
      <div className="space-y-4">
        <div>
          <h3 className="text-base font-medium text-high">
            {t('boardRequired.title')}
          </h3>
          <p className="text-sm text-low mt-1">
            {t('boardRequired.description')}
          </p>
        </div>
      </div>
    );
  }

  if (orgsError) {
    return (
      <div className="py-8">
        <div className="bg-error/10 border border-error/50 rounded-sm p-4 text-error">
          {orgsError instanceof Error
            ? orgsError.message
            : t('settings.loadError')}
        </div>
      </div>
    );
  }

  if (!selectedOrg) {
    return (
      <div className="py-8 text-sm text-low">
        {t('settings.noOrganizations')}
      </div>
    );
  }

  return (
    <>
      {isAdmin ? (
        <AgentUsersCard organizationId={selectedOrg.id} />
      ) : (
        <div className="py-8 text-sm text-low">{t('agents.adminOnly')}</div>
      )}
    </>
  );
}

// Alias for backwards compatibility
export { OrganizationsSettingsSection as OrganizationsSettingsSectionContent };
