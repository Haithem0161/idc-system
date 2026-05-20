import { useEffect, useMemo, useRef } from "react"

/**
 * A debounced callable plus explicit `cancel` and `flush` controls.
 * `cancel` drops any pending invocation; `flush` runs it immediately.
 */
export interface DebouncedCallback<TArgs extends unknown[]> {
  (...args: TArgs): void
  cancel: () => void
  flush: () => void
}

/**
 * Returns a stable function that, when invoked, defers calling the latest
 * version of `fn` until `delayMs` of inactivity has passed. Each new call
 * cancels the pending one. The pending timer is cleared on unmount so the
 * callback never fires after the component goes away.
 */
export function useDebouncedCallback<TArgs extends unknown[]>(
  fn: (...args: TArgs) => void,
  delayMs: number,
): DebouncedCallback<TArgs> {
  const fnRef = useRef(fn)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const pendingArgs = useRef<TArgs | null>(null)

  useEffect(() => {
    fnRef.current = fn
  }, [fn])

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current)
    },
    [],
  )

  return useMemo<DebouncedCallback<TArgs>>(() => {
    const debounced = ((...args: TArgs) => {
      pendingArgs.current = args
      if (timer.current) clearTimeout(timer.current)
      timer.current = setTimeout(() => {
        timer.current = null
        const a = pendingArgs.current
        pendingArgs.current = null
        if (a) fnRef.current(...a)
      }, delayMs)
    }) as DebouncedCallback<TArgs>

    debounced.cancel = () => {
      if (timer.current) clearTimeout(timer.current)
      timer.current = null
      pendingArgs.current = null
    }

    debounced.flush = () => {
      if (timer.current) {
        clearTimeout(timer.current)
        timer.current = null
      }
      const a = pendingArgs.current
      pendingArgs.current = null
      if (a) fnRef.current(...a)
    }

    return debounced
  }, [delayMs])
}
