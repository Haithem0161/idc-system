# Plans

This directory holds development plans for the IDC system. Each plan lives in its own subdirectory and follows the structure defined in [`.claude/rules/planning.md`](../.claude/rules/planning.md).

## Layout

```
docs/
├── README.md                 # this file
├── _template/                # template files to copy when starting a new plan
│   ├── roadmap.md
│   ├── research.md
│   ├── phase-XX.md
│   ├── status.md
│   └── frontend-summary.md
└── <plan-name>/              # one directory per plan
    ├── roadmap.md
    ├── research.md
    ├── phase-01.md
    ├── phase-02.md
    ├── ...
    ├── status.md
    ├── frontend-summary.md
    └── PHASES-X-Y-Z-VERIFICATION.md
```

## Starting a New Plan

```bash
cp -r docs/_template docs/<plan-name>
```

Then fill in `roadmap.md` first (scope, phases, dependencies), then `research.md`, then write each `phase-XX.md` in dependency order. Run gap analysis passes (see `planning.md`) until 0 true gaps remain before starting implementation.

## PRDs vs. Plans

A **PRD** answers *what* and *why*; a **plan** answers *how*. Both can live in the same `docs/<slug>/` directory:

```
docs/<app-or-surface-slug>/
├── PRD-V1.0.0.md            # what we're building, for whom -- see .claude/rules/prd-writing.md
├── Milestone-1.1-<name>.md  # version-by-version deltas to the PRD
├── roadmap.md               # phase breakdown -- see .claude/rules/planning.md
├── research.md
├── phase-01.md              # ...
└── status.md
```

Write a PRD when introducing a new app, sidebar, or major surface (3+ modules). For smaller features (a screen, a field, a workflow inside an existing module), a phase plan alone is enough. See [`.claude/rules/prd-writing.md`](../.claude/rules/prd-writing.md) for the full guideline -- 12 mandatory sections, the offline-aware adaptations, and a definition-of-done checklist.

## Surfaces

Every plan crosses up to three surfaces. Phase files MUST declare which they touch:

- **Frontend** -- React 19 in `src/`.
- **Tauri/Rust** -- desktop runtime + local SQLite in `src-tauri/`.
- **Sync Server** -- Fastify + Prisma in `sync-server/` (when introduced).

A frontend-only plan is allowed but rare. Most non-trivial plans touch at least Frontend + Tauri.
