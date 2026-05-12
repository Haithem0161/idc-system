import { useState } from "react"
import { useNavigate, useParams } from "react-router"
import { useTranslation } from "react-i18next"
import { ArrowLeft, KeyRound, Trash2 } from "lucide-react"
import { Link } from "react-router"

import {
  useUser,
  useUserResetPassword,
  useUserSoftDelete,
  useUserUpdate,
} from "@/features/auth/queries"
import type { UserRoleLiteral } from "@/lib/ipc"
import { cn } from "@/lib/utils"

const ROLE_TONE: Record<UserRoleLiteral, string> = {
  superadmin: "is-superadmin",
  receptionist: "is-receptionist",
  accountant: "is-accountant",
}

export default function UserDetailPage () {
  const { t } = useTranslation()
  const { id = "" } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const query = useUser(id)
  const update = useUserUpdate()
  const reset = useUserResetPassword()
  const remove = useUserSoftDelete()

  const [draft, setDraft] = useState<{
    email: string
    name: string
    role: UserRoleLiteral
    seededFor: string | null
  }>({ email: "", name: "", role: "receptionist", seededFor: null })

  if (query.data && draft.seededFor !== query.data.id) {
    setDraft({
      email: query.data.email,
      name: query.data.name,
      role: query.data.role,
      seededFor: query.data.id,
    })
  }
  const { email, name, role } = draft
  const setEmail = (v: string) => setDraft((d) => ({ ...d, email: v }))
  const setName = (v: string) => setDraft((d) => ({ ...d, name: v }))
  const setRole = (v: UserRoleLiteral) => setDraft((d) => ({ ...d, role: v }))

  const [newPassword, setNewPassword] = useState("")
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)

  if (!query.data) {
    return <p className="text-[13px] text-ink-3">{t("common.loading", { defaultValue: "Loading..." })}</p>
  }

  const submit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    setSuccess(null)
    try {
      await update.mutateAsync({ id, email, name, role })
      setSuccess(t("admin.users.saved", { defaultValue: "Saved" }))
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    }
  }

  const doReset = async () => {
    setError(null)
    setSuccess(null)
    if (newPassword.length < 8) {
      setError(t("admin.users.password_too_short", { defaultValue: "Password must be at least 8 characters" }))
      return
    }
    try {
      await reset.mutateAsync({ id, new_password: newPassword })
      setSuccess(t("admin.users.password_reset_ok", { defaultValue: "Password reset" }))
      setNewPassword("")
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    }
  }

  const doDelete = async () => {
    if (!confirm(t("admin.users.delete_confirm", { defaultValue: "Soft-delete this user?" }))) return
    try {
      await remove.mutateAsync(id)
      navigate("/admin/users", { replace: true })
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    }
  }

  const user = query.data
  const display = user.name?.trim() || user.email
  const initial = (display[0] ?? "?").toUpperCase()

  return (
    <div className="mx-auto max-w-3xl space-y-6">
      <Link
        to="/admin/users"
        className="inline-flex items-center gap-1.5 text-[11.5px] font-semibold uppercase tracking-[0.08em] text-ink-3 transition-colors hover:text-ink"
      >
        <ArrowLeft className="h-3 w-3" strokeWidth={1.8} />
        {t("admin.users.back", { defaultValue: "Back to users" })}
      </Link>

      <header className="flex items-center gap-4 border-b border-line pb-5">
        <div className={cn(
          "flex h-14 w-14 items-center justify-center rounded-xl text-[20px] font-semibold text-white",
          user.role === "superadmin" && "bg-crimson",
          user.role === "accountant" && "bg-gold",
          user.role === "receptionist" && "bg-info",
        )}>
          {initial}
        </div>
        <div className="min-w-0 flex-1 space-y-1">
          <h1 className="truncate text-[22px] font-bold leading-[1.1] tracking-[-0.02em] text-ink" title={display}>
            {display}
          </h1>
          <div className="flex flex-wrap items-center gap-2 text-[11.5px] text-ink-3">
            <span className={cn("role-pill", ROLE_TONE[user.role])}>
              {t(`auth.role_${user.role}`, { defaultValue: user.role })}
            </span>
            <span className="status-pill">
              <span className="font-mono normal-case tracking-normal">{user.id.slice(0, 8)}</span>
            </span>
          </div>
        </div>
      </header>

      <form onSubmit={submit} className="panel">
        <div className="panel-head">
          <span className="panel-title">{t("admin.users.profile", { defaultValue: "Profile" })}</span>
        </div>
        <div className="panel-body space-y-4">
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <Field label={t("auth.email_label", { defaultValue: "Email" })}>
              <input
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                required
                className="input"
              />
            </Field>
            <Field label={t("auth.name_label", { defaultValue: "Name" })}>
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                required
                className="input"
              />
            </Field>
            <Field label={t("admin.users.role", { defaultValue: "Role" })}>
              <select
                value={role}
                onChange={(e) => setRole(e.target.value as UserRoleLiteral)}
                className="input"
              >
                <option value="superadmin">{t("auth.role_superadmin", { defaultValue: "Superadmin" })}</option>
                <option value="receptionist">{t("auth.role_receptionist", { defaultValue: "Receptionist" })}</option>
                <option value="accountant">{t("auth.role_accountant", { defaultValue: "Accountant" })}</option>
              </select>
            </Field>
          </div>
          <div className="flex justify-end">
            <button type="submit" disabled={update.isPending} className="btn btn-ink btn-sm">
              {t("admin.users.save", { defaultValue: "Save" })}
            </button>
          </div>
        </div>
      </form>

      <section className="panel">
        <div className="panel-head">
          <span className="panel-title">{t("admin.users.reset_password", { defaultValue: "Reset password" })}</span>
        </div>
        <div className="panel-body flex flex-wrap items-end gap-3">
          <Field label={t("admin.users.new_password", { defaultValue: "New password" })} className="flex-1 min-w-[200px]">
            <input
              type="password"
              value={newPassword}
              onChange={(e) => setNewPassword(e.target.value)}
              minLength={8}
              className="input"
            />
          </Field>
          <button
            type="button"
            onClick={doReset}
            disabled={reset.isPending}
            className="btn btn-ghost btn-sm"
          >
            <KeyRound className="h-3.5 w-3.5" strokeWidth={1.8} />
            {t("admin.users.reset_password", { defaultValue: "Reset password" })}
          </button>
        </div>
      </section>

      <section className="panel border-crimson/30 bg-crimson-soft/40">
        <div className="panel-body space-y-3">
          <div className="flex items-center justify-between gap-3">
            <div>
              <span className="eyebrow" style={{ color: "var(--crimson)" }}>
                {t("admin.users.danger_zone", { defaultValue: "Danger zone" })}
              </span>
              <p className="mt-1 max-w-md text-[12.5px] text-ink-2">
                {t("admin.users.delete_warning", {
                  defaultValue: "Soft-deletes the user and revokes all active sessions.",
                })}
              </p>
            </div>
            <button
              type="button"
              onClick={doDelete}
              disabled={remove.isPending}
              className="btn btn-danger btn-sm"
            >
              <Trash2 className="h-3.5 w-3.5" strokeWidth={1.8} />
              {t("admin.users.delete", { defaultValue: "Delete user" })}
            </button>
          </div>
        </div>
      </section>

      {error ? (
        <div role="alert" className="status-pill is-danger w-fit">{error}</div>
      ) : null}
      {success ? (
        <div className="status-pill is-success w-fit">{success}</div>
      ) : null}
    </div>
  )
}

function Field ({ label, children, className }: { label: string; children: React.ReactNode; className?: string }) {
  return (
    <label className={cn("block", className)}>
      <span className="field-label">{label}</span>
      {children}
    </label>
  )
}
