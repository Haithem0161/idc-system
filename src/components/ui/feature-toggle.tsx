import { cn } from "@/lib/utils"

interface FeatureToggleProps {
  label: string
  pressed: boolean
  onPressedChange: (pressed: boolean) => void
  disabled?: boolean
  /** Tooltip shown on the disabled state (e.g. "Dye not supported for this check"). */
  disabledHint?: string
}

/**
 * Big pill-style on/off toggle used for primary visit features (dye, report).
 * Renders as `role="switch"` so screen readers announce the pressed state.
 * When disabled, displays `pressed=false` regardless of stored value so the
 * UI never lies about an unsupported feature being on.
 */
export function FeatureToggle ({
  label,
  pressed,
  onPressedChange,
  disabled,
  disabledHint,
}: FeatureToggleProps) {
  const effectivePressed = disabled ? false : pressed
  return (
    <button
      type="button"
      role="switch"
      aria-checked={effectivePressed}
      aria-disabled={disabled || undefined}
      disabled={disabled}
      title={disabled ? disabledHint : undefined}
      onClick={() => {
        if (disabled) return
        onPressedChange(!effectivePressed)
      }}
      className={cn(
        "inline-flex h-11 min-w-[140px] items-center justify-center gap-2 rounded-md border px-5 text-[13px] font-semibold transition-colors",
        effectivePressed
          ? "border-ink bg-ink text-paper shadow-[0_1px_2px_rgba(10,18,48,0.08)]"
          : "border-line-2 bg-paper-2 text-ink-3 hover:bg-paper hover:text-ink",
        disabled && "cursor-not-allowed opacity-50 hover:bg-paper-2 hover:text-ink-3",
      )}
    >
      <span
        aria-hidden="true"
        className={cn(
          "h-1.5 w-1.5 rounded-full",
          effectivePressed ? "bg-paper" : "bg-ink-4",
        )}
      />
      {label}
    </button>
  )
}
