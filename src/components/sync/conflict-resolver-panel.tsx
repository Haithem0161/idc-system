import { useEffect, useState } from "react"
import { useTranslation } from "react-i18next"

import { invoke } from "@/lib/ipc"
import type { Conflict } from "@/lib/schemas/sync"
import { emitToast } from "@/lib/toast"

import { MergeEditor } from "./merge-editor"

type Choice = "local" | "server" | "merged"

interface Props {
  conflict: Conflict
  onResolved: () => void
}

/**
 * Right-side panel: side-by-side payload viewer + Keep-local / Keep-server /
 * Merge actions (phase-08 §4 `<ConflictResolverPanel>` flow).
 *
 * On 409 ALREADY_RESOLVED (phase-08 §7.22), surfaces a toast and triggers
 * the parent's reload (`onResolved`) so the queue refreshes.
 */
export function ConflictResolverPanel({ conflict, onResolved }: Props) {
  const { t } = useTranslation()
  const [choice, setChoice] = useState<Choice>("local")
  const [merged, setMerged] = useState<Record<string, unknown> | null>(null)
  const [submitting, setSubmitting] = useState(false)

  useEffect(() => {
    setChoice("local")
    setMerged(null)
  }, [conflict.opId])

  const submit = async () => {
    setSubmitting(true)
    try {
      const args: { opId: string; choice: Choice; merged?: unknown } = {
        opId: conflict.opId,
        choice,
      }
      if (choice === "merged") {
        if (!merged) {
          emitToast(
            "warning",
            t("sync_conflicts.merge_invalid", {
              defaultValue:
                "Fill in every manual field before submitting the merge.",
            })
          )
          setSubmitting(false)
          return
        }
        args.merged = merged
      }
      await invoke("sync_resolve_conflict", { args })
      emitToast(
        "success",
        t("sync_conflicts.resolved_toast", {
          defaultValue: "Conflict resolved.",
        })
      )
      onResolved()
    } catch (err) {
      const msg = String(err)
      if (msg.includes("ALREADY_RESOLVED")) {
        emitToast(
          "warning",
          t("sync_conflicts.already_resolved", {
            defaultValue:
              "This conflict was already resolved on another device. Refreshing.",
          })
        )
        onResolved()
      } else {
        emitToast(
          "error",
          t("sync_conflicts.resolve_failed", {
            defaultValue: "Failed to resolve conflict: {{msg}}",
            msg,
          })
        )
      }
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="panel">
      <div className="panel-head flex items-center justify-between">
        <div>
          <span className="panel-title">
            {t("sync_conflicts.panel.title", {
              defaultValue: "Resolve conflict",
            })}
          </span>
          <p className="text-[11px] text-ink-3">
            {t(`audit.entities.${conflict.entity}`, { defaultValue: conflict.entity })}{" "}
            · <span className="font-mono">{conflict.entityId.slice(0, 8)}</span>
          </p>
        </div>
        <span className="status-pill is-warn">
          {t(`sync_conflicts.reason.${conflict.reason}`, {
            defaultValue: conflict.reason,
          })}
        </span>
      </div>
      <div className="panel-body space-y-4">
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          <PayloadColumn
            heading={t("sync_conflicts.local", { defaultValue: "Local payload" })}
            tone="info"
            payload={conflict.localPayload}
          />
          <PayloadColumn
            heading={t("sync_conflicts.server", { defaultValue: "Server payload" })}
            tone="success"
            payload={conflict.serverPayload}
          />
        </div>

        <div className="rounded-md border border-line bg-paper-2 p-3">
          <div className="text-[11px] font-semibold uppercase tracking-[0.1em] text-ink-3">
            {t("sync_conflicts.choose", { defaultValue: "Choose resolution" })}
          </div>
          <div className="mt-2 flex flex-wrap gap-2">
            {(["local", "server", "merged"] as Choice[]).map((c) => (
              <button
                key={c}
                type="button"
                onClick={() => setChoice(c)}
                aria-pressed={choice === c}
                className={
                  "btn btn-sm " +
                  (choice === c ? "btn-ink" : "btn-ghost")
                }
              >
                {t(`sync_conflicts.choice.${c}`, { defaultValue: c })}
              </button>
            ))}
          </div>
        </div>

        {choice === "merged" ? (
          <MergeEditor
            local={conflict.localPayload}
            server={conflict.serverPayload}
            onChange={setMerged}
          />
        ) : null}

        <div className="flex justify-end gap-2 border-t border-line pt-3">
          <button
            type="button"
            className="btn btn-primary btn-sm"
            disabled={submitting || (choice === "merged" && !merged)}
            onClick={submit}
          >
            {submitting
              ? t("sync_conflicts.submitting", { defaultValue: "Submitting…" })
              : t("sync_conflicts.submit", { defaultValue: "Submit resolution" })}
          </button>
        </div>
      </div>
    </div>
  )
}

function PayloadColumn({
  heading,
  payload,
  tone,
}: {
  heading: string
  payload: unknown
  tone: "info" | "success"
}) {
  return (
    <div>
      <div
        className={
          "mb-1 text-[10px] font-semibold uppercase tracking-[0.1em] " +
          (tone === "info" ? "text-info" : "text-success")
        }
      >
        {heading}
      </div>
      <pre className="max-h-72 overflow-auto rounded-md border border-line bg-surface p-3 font-mono text-[11px] leading-snug text-ink-2">
        {JSON.stringify(payload, null, 2)}
      </pre>
    </div>
  )
}
