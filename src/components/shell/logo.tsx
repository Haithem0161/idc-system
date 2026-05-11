import { useTranslation } from "react-i18next"

import { cn } from "@/lib/utils"

interface LogoProps {
  className?: string
  /** Width and height in pixels. Defaults to 32. */
  size?: number
  /** Override the alt text. Defaults to the localized app title. */
  alt?: string
}

/**
 * App logo. Serves `logo.webp` with a `logo.png` fallback via `<picture>`.
 * Files live in `public/` (`logo.webp`, `logo.png`, `logo.ico` for favicon).
 */
export function Logo({ className, size = 32, alt }: LogoProps) {
  const { t } = useTranslation()
  const label = alt ?? t("app.title", { defaultValue: "IDC" })
  return (
    <picture>
      <source srcSet="/logo.webp" type="image/webp" />
      <img
        src="/logo.png"
        alt={label}
        width={size}
        height={size}
        className={cn("select-none", className)}
        draggable={false}
      />
    </picture>
  )
}
