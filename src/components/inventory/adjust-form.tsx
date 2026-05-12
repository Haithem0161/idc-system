import { useMemo, useState } from "react"
import { useNavigate } from "react-router"
import { useTranslation } from "react-i18next"

import {
  useInventoryAdjustmentCreate,
  useInventoryItems,
} from "@/features/inventory/queries"
import {
  adjustmentInputSchema,
  toIpcDelta,
  type AdjustmentReasonInput,
} from "@/lib/schemas/inventory"
import { useAuthStore, selectCurrentRole } from "@/stores/auth-store"
import { resolveLocaleName } from "@/lib/format/locale-name"

const LARGE_DELTA_THRESHOLD = 1000

interface Props {
  initialItemId?: string | null
}

/**
 * `<AdjustForm>` -- the operational adjustment form per phase-06 §3 / §7.15.
 *
 * - Hides the `count_correction` radio for non-superadmin users.
 * - Warns when |delta| > 1000 but does not block (phase-06 §7.8).
 * - Submits via `inventory_create_adjustment` IPC.
 */
export function AdjustForm ({ initialItemId }: Props) {
  const { t, i18n } = useTranslation()
  const navigate = useNavigate()
  const locale = (i18n.language?.startsWith("ar") ? "ar" : "en") as "ar" | "en"
  const role = useAuthStore(selectCurrentRole)
  const itemsQuery = useInventoryItems({ include_inactive: false })
  const create = useInventoryAdjustmentCreate()
  const items = itemsQuery.data ?? []

  const [itemId, setItemId] = useState<string>(initialItemId ?? "")
  const [reason, setReason] = useState<AdjustmentReasonInput>("receive")
  const [inputDelta, setInputDelta] = useState<string>("")
  const [note, setNote] = useState<string>("")
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState(false)

  const isSuperadmin = role === "superadmin"
  const reasonChoices: AdjustmentReasonInput[] = isSuperadmin
    ? ["receive", "writeoff", "count_correction"]
    : ["receive", "writeoff"]

  // Lock the reason back to a permitted value if the user just lost the
  // count_correction role mid-form (defensive).
  if (!reasonChoices.includes(reason)) {
    setReason("receive")
  }

  const parsedDelta = useMemo(() => {
    const n = Number(inputDelta)
    return Number.isFinite(n) ? n : null
  }, [inputDelta])

  const showLargeWarning =
    parsedDelta !== null && Math.abs(parsedDelta) > LARGE_DELTA_THRESHOLD

  const submit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    setSuccess(false)

    const candidate = {
      item_id: itemId,
      reason,
      input_delta: parsedDelta ?? Number.NaN,
      note,
    }
    const parsed = adjustmentInputSchema.safeParse(candidate)
    if (!parsed.success) {
      const first = parsed.error.issues[0]
      setError(
        first?.message
          ? t(`inventory.adjust.errors.${first.message}` as const, {
              defaultValue: first.message,
            })
          : t("inventory.adjust.errors.submit_failed")
      )
      return
    }

    try {
      await create.mutateAsync({
        item_id: parsed.data.item_id,
        reason: parsed.data.reason,
        delta: toIpcDelta(parsed.data.reason, parsed.data.input_delta),
        note: parsed.data.note?.length ? parsed.data.note : null,
      })
      setSuccess(true)
      setInputDelta("")
      setNote("")
    } catch (err) {
      const msg = (err as { message?: string }).message ?? ""
      if (/forbidden|requires one of/i.test(msg)) {
        setError(t("inventory.adjust.errors.forbidden"))
      } else {
        setError(msg || t("inventory.adjust.errors.submit_failed"))
      }
    }
  }

  return (
    <form onSubmit={submit} className="panel">
      <div className="panel-head">
        <span className="panel-title">{t("inventory.adjust.title")}</span>
        <div className="text-[12px] text-ink-3 mt-1">
          {t("inventory.adjust.subtitle")}
        </div>
      </div>
      <div className="panel-body space-y-5">
        <div>
          <label className="field-label" htmlFor="adjust-item">
            {t("inventory.adjust.item_label")}
          </label>
          <select
            id="adjust-item"
            value={itemId}
            onChange={(e) => setItemId(e.target.value)}
            className="input"
            required
          >
            <option value="" disabled>
              {t("inventory.adjust.item_placeholder")}
            </option>
            {items.map((item) => (
              <option key={item.id} value={item.id}>
                {resolveLocaleName(item, locale)} ({item.unit})
              </option>
            ))}
          </select>
        </div>

        <div>
          <span className="field-label">
            {t("inventory.adjust.reason_label")}
          </span>
          <div className="flex flex-wrap items-center gap-3">
            {reasonChoices.map((r) => (
              <label
                key={r}
                className={
                  "inline-flex items-center gap-2 cursor-pointer rounded-md border px-3 py-2 text-[12px] " +
                  (reason === r
                    ? "border-ink text-ink bg-paper-2"
                    : "border-line-2 text-ink-3 hover:text-ink-2")
                }
              >
                <input
                  type="radio"
                  name="adjust-reason"
                  value={r}
                  checked={reason === r}
                  onChange={() => setReason(r)}
                  className="h-3.5 w-3.5 accent-ink"
                />
                {t(`inventory.adjust.reasons.${r}` as const)}
              </label>
            ))}
          </div>
        </div>

        <div>
          <label className="field-label" htmlFor="adjust-delta">
            {t(`inventory.adjust.delta_label.${reason}` as const)}
          </label>
          <input
            id="adjust-delta"
            type="number"
            step="1"
            value={inputDelta}
            onChange={(e) => setInputDelta(e.target.value)}
            className="input font-mono"
            required
          />
          <div className="mt-2 text-[12px] text-ink-3">
            {reason === "count_correction"
              ? t("inventory.adjust.helper.count_correction_signed")
              : reason === "writeoff"
                ? t("inventory.adjust.helper.writeoff")
                : t("inventory.adjust.helper.receive")}
          </div>
          {showLargeWarning ? (
            <div className="mt-2 rounded border border-gold bg-gold-soft px-3 py-2 text-[12px] text-gold">
              {t("inventory.adjust.helper.large_warning")}
            </div>
          ) : null}
        </div>

        <div>
          <label className="field-label" htmlFor="adjust-note">
            {t("inventory.adjust.note_label")}
          </label>
          <input
            id="adjust-note"
            type="text"
            value={note}
            onChange={(e) => setNote(e.target.value)}
            className="input"
            placeholder={
              t("inventory.adjust.note_placeholder") as string
            }
            maxLength={500}
          />
        </div>

        {error ? (
          <div className="rounded border border-crimson bg-crimson-soft px-3 py-2 text-[12px] text-crimson">
            {error}
          </div>
        ) : null}
        {success ? (
          <div className="rounded border border-success bg-success-soft px-3 py-2 text-[12px] text-success">
            {t("inventory.adjust.success")}
          </div>
        ) : null}

        <div className="flex flex-wrap items-center justify-end gap-2">
          <button
            type="button"
            onClick={() => navigate("/inventory")}
            className="btn btn-ghost btn-sm"
          >
            {t("inventory.adjust.cancel")}
          </button>
          <button
            type="submit"
            disabled={create.isPending}
            className="btn btn-primary btn-sm"
          >
            {create.isPending
              ? t("inventory.adjust.submitting")
              : t("inventory.adjust.submit")}
          </button>
        </div>
      </div>
    </form>
  )
}
