import { useState } from "react"
import { useTranslation } from "react-i18next"
import { Lock, AlertTriangle } from "lucide-react"

/**
 * Confirmation dialog for "Sign and freeze". Freezing makes a day immutable, so
 * this surfaces the net being frozen and an explicit warning before committing,
 * to prevent freezing the wrong date by accident. Matches the app modal idiom
 * (fixed overlay + .panel).
 */
export function SignCloseDialog ({
  open,
  targetDate,
  netLabel,
  busy,
  onConfirm,
  onClose,
}: {
  open: boolean
  targetDate: string
  netLabel: string
  busy?: boolean
  onConfirm: () => void
  onClose: () => void
}) {
  const { t } = useTranslation()
  if (!open) return null
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-ink/40 p-4">
      <div className="panel w-full max-w-md">
        <div className="panel-head flex items-center justify-between">
          <span className="panel-title">
            {t("accounting.daily_close.sign_confirm.title", { defaultValue: "Sign and freeze" })}
          </span>
          <button
            type="button"
            className="text-ink-3 hover:text-ink"
            onClick={onClose}
            aria-label={t("common.cancel", { defaultValue: "Cancel" })}
          >
            ×
          </button>
        </div>
        <div className="panel-body space-y-4">
          <p className="text-[13px] text-ink-2">
            {t("accounting.daily_close.sign_confirm.body", {
              defaultValue: "Freeze the books for {{date}}? Net {{net}} will be locked in.",
              date: targetDate,
              net: netLabel,
            })}
          </p>
          <div className="flex items-start gap-2 rounded-md border border-gold/30 bg-gold-soft px-3 py-2.5">
            <AlertTriangle className="mt-0.5 h-4 w-4 flex-none text-gold" strokeWidth={1.8} aria-hidden />
            <p className="text-[12px] text-ink-2">
              {t("accounting.daily_close.sign_confirm.warning", {
                defaultValue:
                  "Once frozen, this day becomes immutable: visits dated to it cannot be locked or voided. Only a superadmin can reopen it.",
              })}
            </p>
          </div>
          <div className="flex justify-end gap-2">
            <button type="button" className="btn btn-ghost btn-sm" onClick={onClose} disabled={busy}>
              {t("common.cancel", { defaultValue: "Cancel" })}
            </button>
            <button
              type="button"
              className="btn btn-primary btn-sm"
              onClick={onConfirm}
              disabled={busy}
            >
              <Lock className="h-3.5 w-3.5" strokeWidth={1.8} aria-hidden />
              {busy
                ? t("accounting.daily_close.signing", { defaultValue: "Signing…" })
                : t("accounting.daily_close.sign_confirm.confirm", {
                    defaultValue: "Sign and freeze",
                  })}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}

/**
 * Reopen (unfreeze) dialog: superadmin-only escape hatch. Requires a reason
 * (>= 5 chars, mirroring the void-reason rule) so the audit trail explains why
 * a frozen day was reopened.
 */
export function ReopenCloseDialog ({
  open,
  targetDate,
  busy,
  onConfirm,
  onClose,
}: {
  open: boolean
  targetDate: string
  busy?: boolean
  onConfirm: (reason: string) => void
  onClose: () => void
}) {
  const { t } = useTranslation()
  const [reason, setReason] = useState("")
  if (!open) return null
  const tooShort = reason.trim().length < 5
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-ink/40 p-4">
      <div className="panel w-full max-w-md">
        <div className="panel-head flex items-center justify-between">
          <span className="panel-title">
            {t("accounting.daily_close.reopen_confirm.title", { defaultValue: "Reopen frozen day" })}
          </span>
          <button
            type="button"
            className="text-ink-3 hover:text-ink"
            onClick={onClose}
            aria-label={t("common.cancel", { defaultValue: "Cancel" })}
          >
            ×
          </button>
        </div>
        <div className="panel-body space-y-4">
          <p className="text-[13px] text-ink-2">
            {t("accounting.daily_close.reopen_confirm.body", {
              defaultValue:
                "Reopen {{date}}? This unfreezes the day so visits can be edited again. The action is audited.",
              date: targetDate,
            })}
          </p>
          <div>
            <label className="mb-1 block text-[11px] font-semibold uppercase tracking-[0.08em] text-ink-3">
              {t("accounting.daily_close.reopen_confirm.reason_label", { defaultValue: "Reason" })}
            </label>
            <textarea
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              rows={3}
              className="input w-full text-[13px]"
              placeholder={t("accounting.daily_close.reopen_confirm.reason_placeholder", {
                defaultValue: "Why is this day being reopened?",
              })}
            />
          </div>
          <div className="flex justify-end gap-2">
            <button type="button" className="btn btn-ghost btn-sm" onClick={onClose} disabled={busy}>
              {t("common.cancel", { defaultValue: "Cancel" })}
            </button>
            <button
              type="button"
              className="btn btn-danger btn-sm"
              onClick={() => onConfirm(reason.trim())}
              disabled={busy || tooShort}
            >
              {busy
                ? t("accounting.daily_close.reopening", { defaultValue: "Reopening…" })
                : t("accounting.daily_close.reopen_confirm.confirm", { defaultValue: "Reopen" })}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
