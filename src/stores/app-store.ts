import { create } from "zustand"
import { devtools, persist } from "zustand/middleware"

interface AppState {
  theme: "light" | "dark" | "system"
  setTheme: (theme: "light" | "dark" | "system") => void
}

export const useAppStore = create<AppState>()(
  devtools(
    persist(
      (set) => ({
        theme: "system",
        setTheme: (theme) => set({ theme }),
      }),
      { name: "app-store" },
    ),
  ),
)
