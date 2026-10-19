import { useTranslation } from 'react-i18next';
import { useUserOrganizations } from '@/shared/hooks/useUserOrganizations';
import { useSelectedOrgId } from '@/shared/stores/useOrganizationStore';
import { useAuth } from '@/shared/hooks/auth/useAuth';
import { AgentUsersCard } from '@/shared/components/org/AgentUsersCard';
import { MemberRole as MemberRoleEnum } from 'shared/types';
import { SettingsCard, SettingsLoading } from './SettingsComponents';

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
    return <SettingsLoading label={t('settings.loadingOrganizations')} />;
  }

  if (!hasBoardAccess) {
    return (
      <SettingsCard
        title={t('boardRequired.title')}
        description={t('boardRequired.description')}
      >
        {null}
      </SettingsCard>
    );
  }

  if (orgsError) {
    return (
      <div className="py-8">
        <div className="rounded-lg border border-error/30 bg-error-soft px-4 py-3 text-sm text-error">
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
