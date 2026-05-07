import { useQuery } from "@tanstack/react-query";
import { fetchEmbeddedAuth } from "@/lib/embedded";

/**
 * Hook to poll embedded mode auth status from the Rust HTTP server.
 *
 * This is a convenience hook for components that need embedded auth state
 * outside the AuthProvider. The AuthProvider handles its own polling internally.
 *
 * @param enabled - Whether to enable polling (default: true)
 * @param refetchInterval - Polling interval in ms (default: 5000)
 */
export function useEmbeddedAuth(enabled = true, refetchInterval = 5000) {
  return useQuery({
    queryKey: ["embedded-auth"],
    queryFn: fetchEmbeddedAuth,
    enabled,
    refetchInterval,
    refetchIntervalInBackground: true,
    staleTime: 2000,
  });
}
