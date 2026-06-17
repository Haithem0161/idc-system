import { create } from "zustand"
import { persist, createJSONStorage } from "zustand/middleware"

/**
 * Pre-draft form state that a tab carries before the user has committed
 * enough information (a patient row) to land a real `visits` draft.
 */
export interface VisitTabForm {
  patientId: string | null
  patientName: string
  subtypeId: string | null
  doctorId: string | null
  dye: boolean
  report: boolean
}

export interface VisitTab {
  tabId: string
  checkTypeId: string
  /** Set once a `visits_create_draft` lands; null while the tab is still pre-draft. */
  draftVisitId: string | null
  form: VisitTabForm
}

interface VisitTabsState {
  /** Userid that authored these tabs. Used to clear stale state on user switch. */
  ownerUserId: string | null
  tabs: VisitTab[]
  activeTabId: string | null
  /**
   * A patient stashed by "New visit for this patient" on the patient detail
   * page. The checks grid reads + clears it so the next opened tab binds the
   * patient. Not persisted (transient hand-off only).
   */
  pendingPatient: { id: string; name: string } | null
  setPendingPatient: (p: { id: string; name: string } | null) => void
  openTab: (checkTypeId: string, prefill?: Partial<VisitTabForm>) => string
  closeTab: (tabId: string) => void
  setActiveTab: (tabId: string | null) => void
  updateTabForm: (tabId: string, patch: Partial<VisitTabForm>) => void
  attachDraft: (tabId: string, visitId: string) => void
  /**
   * Drop any tabs whose tabId is listed here. Used by the boot reconciler
   * after it checks each draftVisitId against the DB.
   */
  pruneTabs: (tabIds: string[]) => void
  clearAll: () => void
  /**
   * Claim the store for `userId`. If the persisted state was written by a
   * different user, wipe and adopt. Used at login.
   */
  claimForUser: (userId: string) => void
}

export const VISIT_TAB_CAP = 8

const emptyForm: VisitTabForm = {
  patientId: null,
  patientName: "",
  subtypeId: null,
  doctorId: null,
  dye: false,
  report: false,
}

function makeTabId (): string {
  // Crypto.randomUUID is available in Tauri webview + jsdom (Node 19+).
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID()
  }
  return `tab-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`
}

export const useVisitTabsStore = create<VisitTabsState>()(
  persist(
    (set, get) => ({
      ownerUserId: null,
      tabs: [],
      activeTabId: null,
      pendingPatient: null,

      setPendingPatient: (p) => set({ pendingPatient: p }),

      openTab: (checkTypeId, prefill) => {
        const tabId = makeTabId()
        set((s) => ({
          tabs: [
            ...s.tabs,
            {
              tabId,
              checkTypeId,
              draftVisitId: null,
              form: { ...emptyForm, ...prefill },
            },
          ],
          activeTabId: tabId,
        }))
        return tabId
      },

      closeTab: (tabId) => {
        const { tabs, activeTabId } = get()
        const idx = tabs.findIndex((t) => t.tabId === tabId)
        if (idx === -1) return
        const next = tabs.filter((t) => t.tabId !== tabId)
        let nextActive = activeTabId
        if (activeTabId === tabId) {
          // Activate the neighbour to the right, falling back to the left.
          nextActive = next[idx]?.tabId ?? next[idx - 1]?.tabId ?? null
        }
        set({ tabs: next, activeTabId: nextActive })
      },

      setActiveTab: (tabId) => set({ activeTabId: tabId }),

      updateTabForm: (tabId, patch) =>
        set((s) => ({
          tabs: s.tabs.map((t) =>
            t.tabId === tabId ? { ...t, form: { ...t.form, ...patch } } : t,
          ),
        })),

      attachDraft: (tabId, visitId) =>
        set((s) => ({
          tabs: s.tabs.map((t) =>
            t.tabId === tabId ? { ...t, draftVisitId: visitId } : t,
          ),
        })),

      pruneTabs: (tabIds) => {
        if (tabIds.length === 0) return
        const drop = new Set(tabIds)
        set((s) => {
          const tabs = s.tabs.filter((t) => !drop.has(t.tabId))
          const activeTabId =
            s.activeTabId && drop.has(s.activeTabId)
              ? (tabs[0]?.tabId ?? null)
              : s.activeTabId
          return { tabs, activeTabId }
        })
      },

      clearAll: () => set({ tabs: [], activeTabId: null, ownerUserId: null }),

      claimForUser: (userId) => {
        const { ownerUserId } = get()
        if (ownerUserId === userId) return
        set({ ownerUserId: userId, tabs: [], activeTabId: null })
      },
    }),
    {
      name: "idc.visit-tabs",
      storage: createJSONStorage(() => localStorage),
      // Persist only the structural tab shape and non-PII form fields.
      // Patient-identifying data (patientId, patientName) is deliberately
      // dropped so a logged-in receptionist's in-progress patient names
      // never land in plaintext webview localStorage. On reload the tab is
      // restored empty-of-patient; the receptionist re-enters the name,
      // which the draft (already in SQLite once committed) reconciles.
      partialize: (state) => ({
        ownerUserId: state.ownerUserId,
        activeTabId: state.activeTabId,
        tabs: state.tabs.map((t) => ({
          tabId: t.tabId,
          checkTypeId: t.checkTypeId,
          draftVisitId: t.draftVisitId,
          form: {
            patientId: null,
            patientName: "",
            subtypeId: t.form.subtypeId,
            doctorId: t.form.doctorId,
            dye: t.form.dye,
            report: t.form.report,
          },
        })),
      }),
    },
  ),
)

/** Convenience selector — returns the currently active tab, or null. */
export function selectActiveTab (s: VisitTabsState): VisitTab | null {
  if (!s.activeTabId) return null
  return s.tabs.find((t) => t.tabId === s.activeTabId) ?? null
}
