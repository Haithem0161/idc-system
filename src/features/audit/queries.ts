import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { invoke, isTauri, type AuditQueryArgs } from "@/lib/ipc"
import {
  AuditPageSchema,
  DiagnosticsSummarySchema,
  type AuditPage,
  type DiagnosticsSummary,
} from "@/lib/schemas/audit"

export const auditKeys = {
  query: (filters: AuditQueryArgs) => ["audit", "query", filters] as const,
  diagnostics: ["diagnostics", "summary"] as const,
}

export function useAuditQuery(filters: AuditQueryArgs) {
  return useQuery({
    queryKey: auditKeys.query(filters),
    enabled: isTauri(),
    staleTime: 5_000,
    queryFn: async (): Promise<AuditPage> => {
      const raw = await invoke("audit_query", { args: filters })
      return AuditPageSchema.parse(raw)
    },
  })
}

export function useAuditVacuum() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async () => {
      return await invoke("audit_vacuum_now")
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["audit"] })
    },
  })
}

export function useDiagnosticsSummary() {
  return useQuery({
    queryKey: auditKeys.diagnostics,
    enabled: isTauri(),
    refetchInterval: 30_000,
    queryFn: async (): Promise<DiagnosticsSummary> => {
      const raw = await invoke("diagnostics_summary")
      return DiagnosticsSummarySchema.parse(raw)
    },
  })
}
