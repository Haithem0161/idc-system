import { useState } from "react"
import { Link } from "react-router"
import { useTranslation } from "react-i18next"
import { Plus, X } from "lucide-react"

import { useUserCreate, useUsersList } from "@/features/auth/queries"
import type { UserRoleLiteral } from "@/lib/ipc"
import { cn } from "@/lib/utils"

const ROLE_TONE: Record<UserRoleLiteral, string> = {
  superadmin: "is-superadmin",
  receptionist: "is-receptionist",
  accountant: "is-accountant",
}

export default function UsersListPage () {
  const { t } = useTranslation()
  const [includeInactive, setIncludeInactive] = useState(false)
  const list = useUsersList(includeInactive)
  const create = useUserCreate()

  const [creating, setCreating] = useState(false)
  const [email, setEmail] = useState("")
  const [name, setName] = useState("")
  const [role, setRole] = useState<UserRoleLiteral>("receptionist")
  const [password, setPassword] = useState("")
  const [error, setError] = useState<string | null>(null)

  const total = list.data?.length ?? 0

  const reset = () => {
    setEmail("")
    setName("")
    setRole("receptionist")
    setPassword("")
    setError(null)
    setCreating(false)
  }

  const submit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    try {
      await create.mutateAsync({ email, name, role, password })
      reset()
    } catch (err) {
      setError((err as { message?: string }).message ?? "Failed")
    }
  }

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <header className="flex flex-wrap items-end justify-between gap-3 border-b border-line pb-5">
        <div className="space-y-2">
          <span className="eyebrow">{t("admin.eyebrow", { defaultValue: "Administration" })}</span>
          <h1 className="flex items-center gap-3 text-[28px] font-bold leading-[1.05] tracking-[-0.024em] text-ink">
            {t("admin.users.title", { defaultValue: "Users" })}
            <span className="count-badge text-[11px]">{total}</span>
          </h1>
          <p className="text-[13px] text-ink-3">
            {t("admin.users.subtitle", { defaultValue: "Operators with access to the clinic system." })}
          </p>
        </div>
        <div className="flex items-center gap-3">
          <label className="inline-flex cursor-pointer items-center gap-2 text-[12px] font-medium text-ink-2">
            <input
              type="checkbox"
              checked={includeInactive}
              onChange={(e) => setIncludeInactive(e.target.checked)}
              className="h-3.5 w-3.5 accent-ink"
            />
            <span>{t("admin.users.include_inactive", { defaultValue: "Include inactive" })}</span>
          </label>
          <button
            type="button"
            onClick={() => setCreating((v) => !v)}
            className="btn btn-primary btn-sm"
          >
            {creating ? <X className="h-3.5 w-3.5" strokeWidth={1.8} /> : <Plus className="h-3.5 w-3.5" strokeWidth={1.8} />}
            {creating ? t("admin.users.cancel", { defaultValue: "Cancel" }) : t("admin.users.new", { defaultValue: "New user" })}
          </button>
        </div>
      </header>

      {creating ? (
        <form onSubmit={submit} className="panel">
          <div className="panel-head">
            <span className="panel-title">{t("admin.users.new", { defaultValue: "New user" })}</span>
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
              <Field label={t("auth.password_label", { defaultValue: "Password" })}>
                <input
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  minLength={8}
                  required
                  className="input"
                />
              </Field>
            </div>
            {error ? (
              <div role="alert" className="status-pill is-danger w-full justify-center">{error}</div>
            ) : null}
            <div className="flex justify-end gap-2">
              <button type="button" onClick={reset} className="btn btn-ghost btn-sm">
                {t("admin.users.cancel", { defaultValue: "Cancel" })}
              </button>
              <button type="submit" disabled={create.isPending} className="btn btn-primary btn-sm">
                {t("admin.users.save", { defaultValue: "Save" })}
              </button>
            </div>
          </div>
        </form>
      ) : null}

      <div className="panel overflow-hidden">
        <table className="data-table">
          <thead>
            <tr>
              <th>{t("admin.users.email", { defaultValue: "Email" })}</th>
              <th>{t("admin.users.name", { defaultValue: "Name" })}</th>
              <th>{t("admin.users.role", { defaultValue: "Role" })}</th>
              <th>{t("admin.users.status", { defaultValue: "Status" })}</th>
              <th className="text-end">{t("admin.users.actions", { defaultValue: "Actions" })}</th>
            </tr>
          </thead>
          <tbody>
            {list.data?.map((user) => (
              <tr key={user.id}>
                <td className="font-medium text-ink">{user.email}</td>
                <td>{user.name}</td>
                <td>
                  <span className={cn("role-pill", ROLE_TONE[user.role])}>
                    {t(`auth.role_${user.role}`, { defaultValue: user.role })}
                  </span>
                </td>
                <td>
                  <span className={cn("status-pill", user.is_active ? "is-success" : "")}>
                    {user.is_active
                      ? t("admin.users.active_yes", { defaultValue: "Active" })
                      : t("admin.users.active_no", { defaultValue: "Inactive" })}
                  </span>
                </td>
                <td className="text-end">
                  <Link
                    to={`/admin/users/${user.id}`}
                    className="inline-flex items-center text-[12px] font-medium text-ink-2 underline-offset-4 transition-colors hover:text-crimson hover:underline"
                  >
                    {t("admin.users.edit", { defaultValue: "Edit" })}
                  </Link>
                </td>
              </tr>
            ))}
            {total === 0 ? (
              <tr>
                <td colSpan={5} className="py-12 text-center text-[13px] text-ink-3">
                  {t("admin.users.empty", { defaultValue: "No users yet" })}
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </div>
    </div>
  )
}

function Field ({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="field-label">{label}</span>
      {children}
    </label>
  )
}
