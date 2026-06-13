import { useCallback, useState } from "react"

import { checkForUpdate } from "./updater"

type UpdaterState =
  | { status: "idle" }
  | { status: "checking" }
  | { status: "current" }
  | { status: "unsupported" }
  | { status: "available"; version: string }
  | { status: "installing"; version: string }
  | { status: "error"; message: string }

/// Drives the "Check for updates" UI. Keeps a single in-flight check/install so
/// a double-click can't kick off two downloads, and never leaves the button in a
/// permanent spinner: every path resolves to a terminal state.
export function useUpdater () {
  const [state, setState] = useState<UpdaterState>({ status: "idle" })
  const [install, setInstall] = useState<(() => Promise<void>) | null>(null)

  const runCheck = useCallback(async () => {
    setState({ status: "checking" })
    setInstall(null)
    try {
      const result = await checkForUpdate()
      if (result.kind === "available") {
        setInstall(() => result.install)
        setState({ status: "available", version: result.version })
      } else {
        setState({ status: result.kind === "current" ? "current" : "unsupported" })
      }
    } catch (e) {
      setState({ status: "error", message: String((e as { message?: string }).message ?? e) })
    }
  }, [])

  const runInstall = useCallback(async () => {
    if (!install) return
    const version = state.status === "available" ? state.version : ""
    setState({ status: "installing", version })
    try {
      // On success the app relaunches, so control never returns here.
      await install()
    } catch (e) {
      setState({ status: "error", message: String((e as { message?: string }).message ?? e) })
    }
  }, [install, state])

  return { state, runCheck, runInstall, canInstall: install != null }
}
