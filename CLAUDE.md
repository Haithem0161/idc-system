# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a React 19 + TypeScript + Tauri v2 desktop application template built with Vite and SWC. Designed as a starting point for Torch Business OS apps.

**Always use pnpm** - do not use npm or yarn.

## Commands

```bash
pnpm dev                # Start Vite dev server
pnpm build              # Build frontend (tsc + vite)
pnpm lint               # ESLint check
pnpm preview            # Preview production build
pnpm tauri dev          # Run Tauri desktop app in development
pnpm tauri build        # Build Tauri app for production
```

## Tech Stack

- **Framework:** React 19
- **Build Tool:** Vite 8 with SWC (`@vitejs/plugin-react-swc`)
- **Language:** TypeScript (strict mode, ES2022 target)
- **Styling:** Tailwind CSS v4 (`@tailwindcss/vite`) + shadcn/ui
- **Routing:** React Router v7 (`createBrowserRouter`)
- **State:** Zustand v5 (devtools + persist middleware)
- **Server State:** TanStack React Query v5
- **HTTP:** Axios (custom instance with auth interceptors)
- **i18n:** react-i18next (Arabic/English with RTL support)
- **Animations:** Framer Motion
- **Validation:** Zod v4
- **SEO:** @dr.pogodin/react-helmet
- **Desktop:** Tauri v2 (dual mode: standalone window + Business OS embedded)
- **Rust Backend:** Tokio, Axum, Serde, thiserror, tracing
- **Package Manager:** pnpm
- **Linting:** ESLint 9 with flat config, typescript-eslint, react-hooks plugin

## Architecture

```
src/
├── api/axios.ts            # Axios instance + request/response interceptors (embedded-aware)
├── components/ui/          # shadcn/ui components (add via: pnpx shadcn@latest add <component>)
├── hooks/
│   ├── use-auth.ts         # AuthContext + useAuth hook
│   └── use-embedded-auth.ts # React Query polling hook for embedded mode
├── i18n/
│   ├── index.ts            # i18next config (Arabic + English, auto RTL)
│   └── locales/{en,ar}/    # Translation JSON files
├── lib/
│   ├── embedded.ts         # Embedded mode detection + auth fetch
│   ├── query-client.ts     # React Query client with defaults
│   └── utils.ts            # cn() utility for Tailwind class merging
├── pages/                  # Route page components
├── providers/
│   └── auth-provider.tsx   # AuthProvider (standalone + embedded mode)
├── routes/index.tsx        # createBrowserRouter config
├── stores/                 # Zustand stores
├── App.tsx                 # Root layout (Helmet + Outlet)
├── main.tsx                # Entry: StrictMode → HelmetProvider → QueryClientProvider → AuthProvider → RouterProvider
└── index.css               # Tailwind v4 imports + shadcn/ui CSS variables (light/dark)

src-tauri/
├── Cargo.toml              # Rust dependencies
├── tauri.conf.json         # Tauri config (window, CSP, bundling)
├── capabilities/           # Permission declarations
├── build.rs                # Tauri build script
├── icons/                  # App icons (all platforms)
└── src/
    ├── main.rs             # Entry point → lib::run()
    ├── lib.rs              # Dual mode: standalone Tauri OR embedded runner
    ├── state.rs            # Thread-safe auth state (RwLock)
    ├── error.rs            # AppError enum + AppResult<T>
    └── embedded/           # Business OS integration
        ├── mod.rs          # Embedded mode detection + config
        ├── messages.rs     # IPC protocol types (MessagePack)
        ├── http_server.rs  # Axum: /api/auth + static file serving
        ├── ipc_client.rs   # TCP client for Business OS IPC
        └── runner.rs       # Orchestration (HTTP + IPC + signals)
```

- **Path alias:** `@/` resolves to `src/` (configured in both vite.config.ts and tsconfig.app.json)
- `public/` - Static assets served as-is

## Key Configuration

- **vite.config.ts** - SWC React plugin + Tailwind CSS vite plugin + `@/` path alias + Tauri dev server settings
- **tsconfig.app.json** - Strict TypeScript with no unused variables/parameters + `@/*` paths
- **eslint.config.js** - Flat config format (ESLint 9+)
- **components.json** - shadcn/ui configuration (new-york style, lucide icons)
- **src-tauri/tauri.conf.json** - Tauri app config (window, CSP, bundling, build commands)
- **src-tauri/Cargo.toml** - Rust dependencies

## Reference Guides

- [REACT.md](REACT.md) - React patterns, architecture, and best practices
- [REACT-QUERY.md](REACT-QUERY.md) - TanStack React Query patterns and best practices
- [FRAMER-MOTION.md](FRAMER-MOTION.md) - Framer Motion animation patterns and best practices
- [TAILWIND.md](TAILWIND.md) - Tailwind CSS utilities and patterns
- [I18N.md](I18N.md) - i18n (react-i18next) for Arabic/English translations with RTL support
- [SHADCN.md](SHADCN.md) - shadcn/ui component library and patterns
- [REACT-BITS.md](REACT-BITS.md) - React Bits animated components
- [ZOD.md](ZOD.md) - Zod schema validation and TypeScript types
- [REACT-ROUTER.md](REACT-ROUTER.md) - React Router client-side routing
- [ZUSTAND.md](ZUSTAND.md) - Zustand state management
- [REACT-HELMET.md](REACT-HELMET.md) - React Helmet SEO and meta tags
- [AXIOS.md](AXIOS.md) - Axios HTTP client
- [src-tauri/CLAUDE.md](src-tauri/CLAUDE.md) - Tauri/Rust backend documentation and reference guides

## Tauri Desktop

This template supports dual-mode execution via Tauri v2:

- **Standalone mode**: `pnpm tauri dev` launches a native desktop window with the React frontend
- **Embedded mode**: When launched by Torch Business OS with `TORCH_EMBEDDED_MODE=true`, the app runs headless — serving the frontend via HTTP and communicating with Business OS via MessagePack IPC over TCP

The Rust backend handles auth token management, IPC protocol, and frontend serving. The React frontend detects embedded mode and polls `/api/auth` for auth tokens instead of managing login directly.

See [src-tauri/CLAUDE.md](src-tauri/CLAUDE.md) for the full Rust architecture and 7 reference guides covering Tauri, Tokio, Axum, Serde, thiserror, tracing, and Business OS integration.

<!-- MEMORY:START -->
# Menu

_Last updated: 2026-02-28 | 0 active memories, 0 total_

_For deeper context, use memory_search, memory_related, or memory_ask tools._
<!-- MEMORY:END -->
