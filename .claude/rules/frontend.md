---
paths:
  - "src/**"
  - "vite.config.ts"
  - "tsconfig*.json"
  - "eslint.config.js"
---

# Frontend Rules (React 19 + Vite + TypeScript)

The frontend is React 19 with Vite 8, Tailwind v4, shadcn/ui, React Router v7, Zustand v5, TanStack React Query v5, react-i18next, and Zod v4. It runs in two modes: standalone Tauri webview, and embedded under Torch Business OS via the bundled HTTP server.

## Core Principles

1. **Context7 First (MANDATORY).** Before writing code that uses any library/framework feature -- React 19 hooks, React Router v7 routing, TanStack Query options, Zustand middleware, Zod features, framer-motion APIs, shadcn primitives -- query Context7 for the current version's docs. Never lean on memorized patterns.
2. **No emojis** in code, JSX, comments, commit messages. Use icons from `lucide-react` for visuals; use i18n keys for text.
3. **Strict TypeScript.** `noUnusedLocals`, `noUnusedParameters`, `strict: true`. Treat any `// @ts-expect-error` or `any` as an incident -- comment why and link an issue.
4. **Path alias `@/`.** Always import from `@/...`, never relative paths longer than two levels.
5. **Package management:** never edit `package.json` by hand -- use `pnpm add <pkg>` / `pnpm add -D <pkg>`.

## Reference Guides (auto-loaded for relevant files)

Detailed conventions live in dedicated guides at the repo root. Treat these as the canonical source for each library:

| File | Topic |
|-|-|
| [REACT.md](../../REACT.md) | React 19 patterns, Suspense, server vs client boundaries (n/a here), error boundaries |
| [REACT-QUERY.md](../../REACT-QUERY.md) | Query keys, mutations, optimistic updates, invalidation strategies |
| [REACT-ROUTER.md](../../REACT-ROUTER.md) | `createBrowserRouter`, loaders, actions, lazy routes |
| [ZUSTAND.md](../../ZUSTAND.md) | Slices, persist, devtools, selectors |
| [ZOD.md](../../ZOD.md) | Schemas, inference, transforms, error formatting |
| [I18N.md](../../I18N.md) | i18next setup, RTL switching, namespacing |
| [TAILWIND.md](../../TAILWIND.md) | Utility patterns, design tokens, dark mode |
| [SHADCN.md](../../SHADCN.md) | Adding components (`pnpx shadcn@latest add ...`), composition |
| [FRAMER-MOTION.md](../../FRAMER-MOTION.md) | Variants, layout animation, reduced motion |
| [REACT-HELMET.md](../../REACT-HELMET.md) | Meta and title management |
| [AXIOS.md](../../AXIOS.md) | Instance + interceptors |
| [REACT-BITS.md](../../REACT-BITS.md) | Animated component patterns |

## Directory Layout (authoritative)

```
src/
├── api/                     # axios instance, IPC clients, sync client
├── components/
│   ├── ui/                  # shadcn primitives (managed via shadcn CLI)
│   └── <domain>/            # domain-scoped components (forms, tables, dialogs)
├── features/<domain>/       # feature modules (queries, mutations, hooks, schemas)
├── hooks/                   # cross-cutting hooks (use-auth, use-online, etc.)
├── i18n/locales/{en,ar}/    # translation JSONs
├── lib/
│   ├── ipc.ts               # typed wrapper around invoke<>()
│   ├── query-client.ts      # React Query defaults
│   ├── schemas/             # shared Zod schemas
│   └── utils.ts             # cn() etc.
├── pages/                   # route page components (one file per route)
├── providers/               # Auth, Theme, Online status, Sync status
├── routes/index.tsx         # createBrowserRouter config
├── stores/                  # Zustand stores
├── App.tsx
├── main.tsx
└── index.css
```

A new screen lives in `pages/`; its data + business hooks live under `features/<domain>/`. UI atoms come from `components/ui/`; domain UI from `components/<domain>/`.

## State Architecture

| Concern | Tool |
|-|-|
| Server state (sync server, IPC results) | TanStack React Query |
| Local UI state | `useState` / `useReducer` |
| Cross-component client state (auth, theme, sync status, draft forms) | Zustand |
| Form state | React Hook Form + Zod resolver |
| Validation | Zod (single source of truth, infer types with `z.infer`) |

**Never** mirror server state in Zustand. The cache for an entity is its React Query key.

## IPC and Data Flow

- All Rust calls go through a typed `invoke<TArgs, TResult>(command, args)` helper in `src/lib/ipc.ts`. The helper enforces matching command-name, argument shape, and return shape via TypeScript.
- IPC results feed React Query via dedicated hooks in `features/<domain>/queries.ts`.
- Mutations apply optimistic updates against the local cache, then invoke the Tauri command, then `queryClient.invalidateQueries(<key>)` on settle.
- The frontend MUST NOT call the sync server directly except for auth login/refresh and explicit health endpoints. Everything else flows: UI -> Tauri command -> SQLite -> sync engine -> server.

## Auth

- `AuthProvider` ships in two modes: standalone (login screen + cached JWT) and embedded (poll `/api/auth` until token arrives).
- Token storage: in-memory + Tauri secure storage (`@tauri-apps/plugin-stronghold` or app data dir behind a Rust command). Never `localStorage`/`sessionStorage` for tokens in standalone mode.
- Axios interceptor attaches `Authorization: Bearer <token>` and refreshes on 401 by re-invoking the auth IPC command.

## i18n

- Default locale: English. Arabic is fully supported with RTL.
- All user-facing strings come from `src/i18n/locales/{en,ar}/<namespace>.json`. No string literals in JSX outside `<Trans>` / `t()`.
- The locale toggle flips `<html dir>` between `ltr`/`rtl`. Test every new screen in both directions.

## Performance

- Lazy-load route components (`React.lazy` + Suspense fallback).
- Set project-wide defaults in `src/lib/query-client.ts` (the library defaults are `staleTime: 0` and `gcTime: 5 * 60 * 1000`). Recommended project defaults: `staleTime: 30_000`, `gcTime: 5 * 60_000`. Override per-query when the data is local-first (then `staleTime: Infinity` is acceptable since invalidation is explicit).
- Tables: virtualize beyond ~200 rows (TanStack Virtual).
- Bundle: Vite handles code splitting by route automatically; verify with `pnpm build` and `dist/assets` size.

## Linting and Types

- `pnpm lint` must pass before commit.
- `pnpm build` runs `tsc -b` first; type errors block the build.
- Prefer `import type` for type-only imports.
- React 19: do not use `forwardRef` for new components; pass `ref` as a prop.

## Common Pitfalls

- React Router v7 file-routes vs `createBrowserRouter`: this template uses `createBrowserRouter`. Don't introduce file-based routing without an architectural decision.
- React 19 + `useEffect`: avoid effects for derived state (memoize) and for fetching (use React Query).
- Tailwind v4 uses a CSS-first config; design tokens live in `src/index.css` `@theme` block, not `tailwind.config.js`.
- shadcn components are owned in-tree -- edit `components/ui/<file>.tsx` directly when needed; do not wrap in unnecessary abstractions.
- Zustand persist middleware: never persist auth tokens; persist only UI prefs (theme, locale, last-opened tab).
