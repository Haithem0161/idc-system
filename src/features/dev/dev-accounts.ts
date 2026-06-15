// Dev-only account switcher (import.meta.env.DEV gated by the consumer).
//
// Lets a developer flip between the three role surfaces (admin / reception /
// accounting) WITHOUT manually logging out and back in. It does a REAL re-login
// as a per-role dev account, so each view is a genuine session for that role --
// true to production, not a faked client-side role override.
//
// The three accounts are fixed, known dev credentials. Missing accounts are
// created lazily via the superadmin-only `users_create`, so they must be
// provisioned WHILE signed in as the superadmin (the admin slot). The switcher
// UI ensures all three exist up front before offering to switch away.

import { invoke } from "@/lib/ipc"
import type { UserRoleLiteral, AuthLoginResult, UserAdminRecord } from "@/lib/ipc"

export interface DevAccount {
  readonly role: UserRoleLiteral
  readonly email: string
  readonly name: string
  readonly password: string
}

// Fixed dev credentials. Password is >=8 chars (the bootstrap/users_create
// invariant). These only ever exist in dev builds.
export const DEV_ACCOUNTS: readonly DevAccount[] = [
  { role: "superadmin", email: "dev-admin@idc.local", name: "Dev Admin", password: "devpass123" },
  { role: "receptionist", email: "dev-reception@idc.local", name: "Dev Reception", password: "devpass123" },
  { role: "accountant", email: "dev-accounting@idc.local", name: "Dev Accounting", password: "devpass123" },
]

export function devAccountForRole (role: UserRoleLiteral): DevAccount | undefined {
  return DEV_ACCOUNTS.find((a) => a.role === role)
}

/**
 * Create any of the three dev accounts that don't exist yet, INCLUDING the
 * dev-admin superadmin (a superadmin may create another superadmin -- only the
 * actor must be one). Requires the caller to currently be a superadmin
 * (users_create is superadmin-only), so the UI calls this while you are signed
 * in as your first-run admin, before offering to switch away. Safe to call
 * repeatedly -- existing accounts are skipped. Returns the emails created.
 */
export async function ensureDevAccounts (): Promise<string[]> {
  const existing = (await invoke("users_list", { args: { include_inactive: true } })) as UserAdminRecord[]
  const byEmail = new Set(existing.map((u) => u.email.toLowerCase()))
  const created: string[] = []
  for (const acct of DEV_ACCOUNTS) {
    if (byEmail.has(acct.email.toLowerCase())) continue
    await invoke("users_create", {
      args: { email: acct.email, name: acct.name, role: acct.role, password: acct.password },
    })
    created.push(acct.email)
  }
  // CRITICAL: users_create writes locally + queues an outbox push (WITH the
  // password hash). Login is SERVER-authoritative, so a just-created account
  // can't log in online until that push reaches the server. Push now and wait
  // for the outbox to drain before any switch attempts to log in as them.
  if (created.length > 0) {
    await pushAndDrain()
  }
  return created
}

/**
 * Force a sync push and wait (best-effort, bounded) for the outbox to empty, so
 * locally-created dev accounts exist on the server before we try to log in as
 * them. Returns once the outbox is empty or the timeout elapses.
 */
async function pushAndDrain (timeoutMs = 20000): Promise<void> {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    // Re-trigger every poll: a just-set token + freshly-enqueued ops may need
    // the engine to be nudged again once it has both.
    await invoke("sync_trigger_push").catch(() => undefined)
    const n = (await invoke("sync_outbox_count").catch(() => 1)) as number
    if (typeof n === "number" && n === 0) return
    await new Promise((r) => setTimeout(r, 400))
  }
  // Timed out with ops still pending -- the caller (switcher) will still try the
  // login; if the account isn't on the server yet the login throws and the
  // current session stands (switchToRole no longer logs out first).
}

/** The fixed dev credential to log in as for a target role. */
export function credentialForRole (role: UserRoleLiteral): DevAccount | undefined {
  return devAccountForRole(role)
}

/**
 * Perform a real account switch by logging in as the dev account for `role`.
 *
 * Crucially we do NOT log out first: `auth_login` REPLACES the current session,
 * so if the target login fails (e.g. the account hasn't reached the server yet)
 * the caller is still authenticated as whoever they were -- never stranded on
 * the login screen. (The previous design logged out first, which is exactly
 * what stranded the user when the dev account wasn't on the server.)
 *
 * Throws on auth failure so the UI can surface it; the existing session stands.
 */
export async function switchToRole (role: UserRoleLiteral): Promise<AuthLoginResult> {
  const acct = credentialForRole(role)
  if (!acct) throw new Error(`no dev account configured for role ${role}`)
  const result = (await invoke("auth_login", {
    args: { email: acct.email, password: acct.password },
  })) as AuthLoginResult
  return result
}

/** The role's landing route, mirroring routes/index/redirect.tsx. */
export function homeForRole (role: UserRoleLiteral): string {
  return role === "accountant" ? "/accounting" : "/reception"
}
