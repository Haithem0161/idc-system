import { useEffect, useState } from "react"

/**
 * Returns a debounced copy of `value` that only updates after `delayMs` of no
 * changes. Use to throttle search-as-you-type queries without re-fetching on
 * every keystroke (the input stays responsive; the debounced value drives the
 * query).
 */
export function useDebouncedValue<T> (value: T, delayMs = 250): T {
  const [debounced, setDebounced] = useState(value)
  useEffect(() => {
    const id = window.setTimeout(() => setDebounced(value), delayMs)
    return () => window.clearTimeout(id)
  }, [value, delayMs])
  return debounced
}
