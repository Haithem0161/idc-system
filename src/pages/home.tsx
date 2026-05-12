import { useTranslation } from "react-i18next"

import { useAuthStore } from "@/stores/auth-store"

export default function HomePage() {
  const { t, i18n } = useTranslation()
  const state = useAuthStore((s) => s.state)

  const today = new Date()
  const dateStamp = today.toLocaleDateString(i18n.language === "ar" ? "ar" : "en-US", {
    weekday: "long",
    day: "numeric",
    month: "long",
    year: "numeric",
  }).toUpperCase()
  const timeStamp = today.toLocaleTimeString(i18n.language === "ar" ? "ar" : "en-US", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  })

  const display = state.kind === "authenticated" ? (state.user.name?.trim() || state.user.email) : ""
  const firstName = display.split(" ")[0] ?? display

  return (
    <div className="mx-auto max-w-5xl space-y-10">
      <header className="space-y-3 border-b border-line pb-6">
        <span className="eyebrow">{dateStamp} · {timeStamp}</span>
        <h1 className="text-[30px] font-bold leading-[1.05] tracking-[-0.026em] text-ink">
          {firstName
            ? t("home.greeting", { name: firstName, defaultValue: `Good morning, ${firstName}.` })
            : t("app.title", { defaultValue: "IDC System" })}
        </h1>
        <p className="max-w-2xl text-[13px] text-ink-3">
          {t("home.subtitle", {
            defaultValue: "Offline-first clinic operations. The rest of the workspace lights up as phases ship.",
          })}
        </p>
      </header>

      <section className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        <PhaseCard
          phase="01"
          title={t("home.phase_01_title", { defaultValue: "Foundation & sync" })}
          status="done"
          body={t("home.phase_01_body", {
            defaultValue: "SQLite, outbox, audit log, push/pull engine.",
          })}
        />
        <PhaseCard
          phase="02"
          title={t("home.phase_02_title", { defaultValue: "Authentication & users" })}
          status="done"
          body={t("home.phase_02_body", {
            defaultValue: "JWT, roles, settings, idle lock, first-run bootstrap.",
          })}
        />
        <PhaseCard
          phase="03"
          title={t("home.phase_03_title", { defaultValue: "Catalog & reference data" })}
          status="next"
          body={t("home.phase_03_body", {
            defaultValue: "Doctors, check types, pricing, operators.",
          })}
        />
      </section>
    </div>
  )
}

function PhaseCard({
  phase,
  title,
  status,
  body,
}: {
  phase: string
  title: string
  status: "done" | "next" | "later"
  body: string
}) {
  const { t } = useTranslation()
  const tone =
    status === "done" ? "is-success" : status === "next" ? "is-warn" : ""
  const label =
    status === "done"
      ? t("home.status_done", { defaultValue: "Shipped" })
      : status === "next"
      ? t("home.status_next", { defaultValue: "Up next" })
      : t("home.status_later", { defaultValue: "Planned" })

  return (
    <article className="panel">
      <div className="panel-body space-y-3">
        <div className="flex items-center justify-between">
          <span className="font-mono text-[11px] uppercase tracking-[0.12em] text-ink-3">
            Phase {phase}
          </span>
          <span className={`status-pill ${tone}`}>{label}</span>
        </div>
        <h3 className="text-[15px] font-semibold leading-[1.2] tracking-[-0.01em] text-ink">{title}</h3>
        <p className="text-[12.5px] leading-normal text-ink-3">{body}</p>
      </div>
    </article>
  )
}
