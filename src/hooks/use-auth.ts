import { createContext, useContext } from "react";

export interface AuthUser {
  userId: string;
  entityId: string;
  email: string;
  name: string | null;
  role: string;
}

export interface AuthContextValue {
  token: string | null;
  isAuthenticated: boolean;
  isEmbedded: boolean;
  isLoading: boolean;
  user: AuthUser | null;
}

export const AuthContext = createContext<AuthContextValue>({
  token: null,
  isAuthenticated: false,
  isEmbedded: false,
  isLoading: true,
  user: null,
});

export const useAuth = () => useContext(AuthContext);
