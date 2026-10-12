import { useMemo, type ReactNode } from 'react';
import {
  AuthContext,
  type AuthContextValue,
} from '@/shared/hooks/auth/useAuth';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { hasBoardAccess } from '@/shared/lib/auth/boardStatus';

interface LocalAuthProviderProps {
  children: ReactNode;
}

export function LocalAuthProvider({ children }: LocalAuthProviderProps) {
  const { boardStatus } = useUserSystem();

  const value = useMemo<AuthContextValue>(
    () => ({
      hasBoardAccess: hasBoardAccess(boardStatus),
      isLoaded: boardStatus !== null,
      userId:
        boardStatus?.status === 'connected'
          ? boardStatus.profile.user_id
          : null,
    }),
    [boardStatus]
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}
