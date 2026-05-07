# <Plan Name> -- Roadmap

**Started:** YYYY-MM-DD
**Target:** <one-sentence description of what this plan delivers>
**Scope:**
- Local entities: N
- Server entities: N
- IPC commands: N
- HTTP routes: N
- Sync contracts: N
- Reports / screens: N

## Phase Overview

| # | Phase | Surfaces | Scope | Size | Depends On | Status |
|-|-|-|-|-|-|-|
| 01 | <name> | Frontend / Tauri / Server / All | <one-line scope> | S/M/L/XL | None | not-started |
| 02 | <name> | ... | ... | ... | 01 | not-started |

## Dependency Graph

```
01 ── 02 ── 04
       └── 03
```

(Replace with actual ASCII art of the dependency graph; show parallel tracks.)

## New Local Entities by Phase

| Phase | Local Tables (SQLite) |
|-|-|
| 01 | `<table_a>`, `<table_b>` |
| 02 | `<table_c>` |

## New Server Entities by Phase

| Phase | Server Models (Prisma) |
|-|-|
| 01 | `<ModelA>`, `<ModelB>` |

## New Business Engines by Phase

| Phase | Frontend services | Rust domain services | Server domain services |
|-|-|-|-|
| 01 | `useThing` | `ThingService` (Rust) | `ThingService` (TS) |

## Sync Contracts by Phase

| Phase | Entity | Push | Pull | Conflict Policy |
|-|-|-|-|-|
| 01 | `thing` | yes | yes | last-write-wins |

## Gap Analysis Log

### Pass 1 (YYYY-MM-DD)
- Gaps found: 0
- Categories: -
- Distribution: -

(Append subsequent passes here.)
