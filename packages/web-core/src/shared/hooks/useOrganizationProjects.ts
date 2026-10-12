import { useShape } from '@/shared/integrations/electric/hooks';
import { PROJECTS_SHAPE } from 'shared/remote-types';
import { useAuth } from '@/shared/hooks/auth/useAuth';

export function useOrganizationProjects(organizationId: string | null) {
  const { hasBoardAccess } = useAuth();

  // Only subscribe to Electric when signed in AND have an org
  const enabled = hasBoardAccess && !!organizationId;

  const { data, isLoading, error } = useShape(
    PROJECTS_SHAPE,
    { organization_id: organizationId || '' },
    { enabled }
  );

  return {
    data,
    isLoading,
    isError: !!error,
    error,
  };
}
