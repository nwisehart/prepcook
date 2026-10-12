import { useContext } from 'react';
import { createHmrContext } from '@/shared/lib/hmrContext';

export interface AuthContextValue {
  /** The server holds a board URL and a board secret. */
  hasBoardAccess: boolean;
  isLoaded: boolean;
  /** The board owner's user id, once the board has answered. */
  userId: string | null;
}

export const AuthContext = createHmrContext<AuthContextValue | undefined>(
  'AuthContext',
  undefined
);

export function useAuth(): AuthContextValue {
  const context = useContext(AuthContext);
  if (context === undefined) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return context;
}
