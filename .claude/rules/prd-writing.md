---
paths:
  - "docs/**/*PRD*.md"
  - "docs/**/PRD-*.md"
  - "docs/**/prd.md"
  - "docs/**/*Milestone*.md"
---

# PRD Writing Rules

A PRD (Product Requirements Document) is the source of truth for **what** an app or major surface delivers, **why**, and **for whom**. Engineering plans (`docs/<plan-name>/phase-XX.md` per [`planning.md`](./planning.md)) tell us **how**. Never duplicate -- a phase file references the PRD; a PRD never tells engineers which migrations to write.

This rule codifies the structure used across the Torch Business OS PRDs (Chat, Contact Center, Finance, Organization Setup, Import Engine, E-Commerce) and adapts it for the IDC system's offline-first reality.

## When to Write a PRD vs. Just a Plan

| Situation | Artifact |
|-|-|
| New app or major sidebar / surface (3+ modules, novel domain) | **PRD** + per-milestone phase plans. |
| New milestone / version inside an existing app (adds modules, changes scope) | **Milestone PRD** (`<App>_Milestone_X.Y_Name.md`) referencing the parent app PRD. |
| Adding a feature to an existing module (new screen, new field, small workflow) | **Phase plan only**. Add a Section 7+ entry to the relevant PRD only if scope changes. |
| Bugfix, refactor, infrastructure-only work | **No PRD.** Phase plan or commit message is enough. |

If in doubt, write the smaller artifact. PRDs are expensive to maintain; never start one to feel productive.

## File Naming and Location

```
docs/
└── <app-or-surface-slug>/
    ├── PRD-V<MAJOR>.<MINOR>.<PATCH>.md     # main PRD, semver
    ├── Milestone-<X>.<Y>-<name>.md         # milestone deltas
    ├── research.md                          # see planning.md
    └── phase-XX.md, status.md, ...          # see planning.md
```

- Slug is kebab-case (`document-center`, `contact-center`, `import-engine`).
- PRD versions use semver (`1.10.2` is allowed). Bump MAJOR for incompatible scope changes, MINOR for additions, PATCH for clarifications/typos.
- One **active** version of a PRD at a time. Older versions live in git history -- do NOT keep `PRD-V1.0.0.md`, `PRD-V1.1.0.md` side-by-side.
- Milestone PRDs are append-only deltas, named `Milestone-1.1-Foundation.md`, `Milestone-2.1-Core-Completeness.md`, etc.

## Required Sections (App PRD)

All sections numbered with the `§` symbol or plain Arabic numerals -- pick one and be consistent within a document. Section count: 12 mandatory sections + Glossary.

### §0 Document History

```
| Version | Date | Author | Status | Notes |
|-|-|-|-|-|
| 0.1.0 | YYYY-MM-DD | <team or person> | Draft / Approved / Superseded | Brief change summary -- one paragraph max. |
```

Plus `### Precedent Documents` listing other PRDs / code references this PRD borrows structure from. This makes review faster and forces conscious reuse.

### §1 Executive Summary

| Subsection | Content |
|-|-|
| §1.1 Overview | 2-4 paragraphs. What surface this is, where it lives, its primary purpose. NO implementation detail. |
| §1.2 Key Objectives | Numbered list of 5-10 outcome-level objectives. Each starts with an action verb. |
| §1.3 Success Metrics | Table: `Metric \| Target \| Measurement Method`. Targets MUST be quantitative (latency p95, conversion rate, retention). Vague metrics like "improve UX" are forbidden. |
| §1.4 Scope Boundaries | Three-column table: `In Scope \| Out of Scope \| Rationale`. Every Out-of-Scope row needs a Rationale -- "why later" or "owned elsewhere". |
| §1.5 Target Users & Personas | One persona per primary role. Format: `Name (Role) \| Key Needs \| Use Case`. Secondary personas listed separately. |
| §1.6 Technology Stack | Table referencing the relevant `.claude/rules/*.md` files. Only call out app-specific deviations from the stack. |

### §2 Module Packaging & Entitlements

For IDC, this is simpler than Torch (no module marketplace), but still required:

| Subsection | Content |
|-|-|
| §2.1 Package Definitions | Free vs paid? Bundled vs separate install? Per-seat vs per-org? For IDC's first releases this is usually "single bundled app, no entitlements". Say so explicitly. |
| §2.2 Entitlement Behavior | What a user sees if they don't have access. (For IDC: usually "all users have access".) |
| §2.3 Surface Summary | Where this lives in the app shell -- main sidebar, settings sub-page, embedded widget, status-bar item. |

### §3 Application Architecture

| Subsection | Content |
|-|-|
| §3.1 Navigation Tree | ASCII tree of routes / pages with their parent-child structure. |
| §3.2 Page Count Summary | Table: `Module \| Pages \| Notes`. Aggregate page count drives milestone sizing. |
| §3.3 Navigation Pattern | Master-detail? Tabs? Wizard? Reference the existing IDC patterns -- if you're inventing a new one, justify why. |

### §4 Core Architectural Patterns

App-specific patterns. Examples: "macOS System Settings pattern" (Org Setup), "Embedded Widget" (Chat), "Wizard Flow" (Import). Name each pattern, describe it, and link to a code reference when one exists. Generic platform patterns belong in `.claude/rules/`, not here.

### §5 Surface Integration

For IDC, replace the Torch "Platform Service Integration" with surface-specific integration:

| Integration | Cover |
|-|-|
| §5.1 Tauri / Rust | Required IPC commands, capabilities, plugins, secure-storage usage. |
| §5.2 Sync Server | Which sync endpoints this surface uses; backup contracts. |
| §5.3 Embedded Mode (Business OS) | If the surface ships in BOS mode, what hooks it exposes. Otherwise: "N/A". |
| §5.4 Document Center / Storage | If the surface produces or consumes files. |
| §5.5 Auth | Which JWT claims it relies on, RBAC requirements, offline auth behavior. |

Each subsection lists the **contract** (what the surface needs from the integration) and the **boundary** (what the surface does NOT do, deferring to the other side).

### §6 Data Model

The hardest section to write well. Two parallel representations are MANDATORY for any syncable entity:

**§6.1 Entity Definitions**

For each entity, in order:

1. **One-paragraph purpose.** Plain English. What this entity is and what it represents in the user's mental model.
2. **Core Fields** table: `Field \| Type \| Required \| Searchable \| Notes`.
3. **Extended / Profile Fields** table (if any).
4. **Timestamps & Audit** table -- standard columns: `created_at`, `updated_at`, `deleted_at`, `created_by`, `updated_by`. For syncable entities also: `version`, `last_synced_at`, `origin_device_id` (per [`offline-first.md`](./offline-first.md)).
5. **Local Schema (SQLite)** -- copy-paste-ready `CREATE TABLE` SQL.
6. **Server Schema (Prisma)** -- copy-paste-ready Prisma model block.
7. **Invariants** -- numbered list of rules the entity enforces (uniqueness, referential, business).
8. **State Machine** -- ASCII diagram if the entity has lifecycle states. Plus a transition table: `From \| To \| Trigger \| Side Effects`.
9. **Sync Policy** -- one of `last-write-wins`, `field-merge`, `additive-only`, `manual`. Required for every syncable entity. State why.

**§6.2 Cross-App / Cross-Surface References**

Table: `Entity \| Owner \| Consumer \| Contract`. Lists references this app holds to entities owned elsewhere (foreign keys to `users`, `entities`, etc.) and the contract the consumer can rely on.

**§6.3 Entity Relationship Map**

ASCII or Mermaid diagram. If you can't fit it in 80 columns, split into per-area diagrams.

### §7 Module Specifications

The biggest section. One subsection per module (`§7.1 <Module Name>`). Each module has:

- **Purpose** -- 1-2 sentences.
- **List Page** spec: layout sketch, columns table, filters, search, sort, bulk actions, pagination.
- **Detail Page** spec: tabs, sections, fields shown, inline edit affordances.
- **Create / Edit Forms**: fields, validation rules (link to Zod schemas in code), required vs optional.
- **Actions** table: `Action \| Trigger \| Permission \| Side Effects \| Audit Event`.
- **Empty States, Error States, Loading States** -- explicitly designed.
- **Mobile / Compact behavior** if applicable.

Use ASCII layout sketches generously -- they are cheap to draw and prevent ambiguity.

### §8 Cross-Module Business Logic

Workflows that span multiple modules or entities. Each workflow:

```
| Property | Description |
|-|-|
| Trigger | What initiates this workflow. |
| Surfaces Involved | Which modules / pages participate. |
| Frequency | Once per X / on every Y / scheduled. |
```

Followed by a numbered **Step Sequence** (ASCII flow allowed for branching). Followed by **Business Rules** -- bullet list of invariants.

If the workflow is offline-aware, list:
- What happens if the user is offline at each step.
- What gets queued.
- How the UI signals "pending sync".

### §9 Multi-User & Multi-Tenant Support

Where Torch had "Multi-Branch", IDC has "Multi-User on a Single Device" and "Multi-Tenant on the Sync Server" (when a tenant equals a customer's organization). Cover:

| Subsection | Content |
|-|-|
| §9.1 Tenant Scoping | Server-side `entityId` injection. Which tables in TENANT_MODELS. |
| §9.2 Multi-User Local Behavior | If multiple human users share the same device install, how is per-user data scoped? Per-user SQLite file? Logical scope on a single DB? Decide and document. |
| §9.3 Cross-Device Behavior | Same user on two devices: what's synced live, what's per-device (UI prefs, drafts, last-opened tab). |

### §10 System Features

Cross-cutting features the surface inherits from the platform:

| Subsection | Content |
|-|-|
| §10.1 Search | Local FTS strategy (SQLite `fts5`?), server-side search if any. |
| §10.2 Export / Import | What can be exported, formats, GDPR concerns. |
| §10.3 Printing | Templates, page layout, header/footer. Document Center handoff if applicable. |
| §10.4 Audit | What gets audit-logged, retention. |
| §10.5 Multi-Currency | Only if relevant to the surface. |
| §10.6 Localization | What strings, what date/number formats, what locales. |
| §10.7 Accessibility (WCAG 2.1 AA) | Keyboard navigation, screen reader, contrast. |
| §10.8 Offline UX | How the surface signals offline / pending sync / sync error to the user. **Required**. |

### §11 Future Enhancements

Three-tier horizon:

| Horizon | Definition |
|-|-|
| Horizon 1 | Next minor version. Well-defined, likely soon (1-2 milestones out). |
| Horizon 2 | Next major version. Requires significant planning. |
| Horizon 3 | Long-term vision. Aspirational, may never ship. |

Plus a **Considered & Rejected** subsection: ideas explicitly excluded with rationale. This is gold for future maintainers.

### §12 Glossary

Alphabetical. Every domain term that isn't standard English. Include cross-references.

---

## Style Rules

1. **No emojis** -- ever. Not in headings, not in tables, not in ASCII art. Use `lucide-react` icons in the actual UI; in PRDs, use words ("warning indicator" rather than a triangle glyph).
2. **Tables over prose for any list of 4+ items.** Markdown tables, minimum separator (`|-|-|`).
3. **ASCII art for diagrams.** 80-column max. Use `─` set sparingly -- prefer plain ASCII (`+--+`, `|`, `->`, `<-`) for portability. Mermaid is acceptable for complex graphs.
4. **Numbered scope.** Every requirement gets a stable number (`§7.1.3`) so phase plans can reference it.
5. **Be specific.** "Fast" is wrong; "p95 < 100ms intra-region" is right. "Easy to use" is wrong; "completes in 3 clicks from any entry point" is right.
6. **Quantify out-of-scope.** Don't write "we won't do MFA"; write "MFA is owned by the auth-service and out of this PRD's scope".
7. **No marketing language.** PRDs are engineering artifacts. "Revolutionary, best-in-class" goes in pitch decks.
8. **Cite code where it exists.** A reference to `apps/<service>/src/.../foo.ts` is worth a paragraph of description.
9. **One concept per sentence.** If a sentence has more than two commas, split it.
10. **Date format.** Always `YYYY-MM-DD` in tables and headers. No locale-dependent formats.
11. **Currency.** Always state the unit explicitly (`USD`, `IQD`). Never bare numbers for money.
12. **Casing.** PascalCase for entity names in prose (`Voucher`, `Room`), `snake_case` for column names, `kebab-case` for slugs and route paths.

## Quality Bar (Definition of Done)

A PRD is ready for engineering when:

- [ ] All 12 sections present and non-empty.
- [ ] Every entity in §6 has both Local SQLite and Server Prisma schemas.
- [ ] Every syncable entity in §6 declares a sync policy.
- [ ] Every workflow in §8 declares its offline behavior.
- [ ] §10.8 (Offline UX) is filled in -- not a stub.
- [ ] Success metrics in §1.3 are quantitative.
- [ ] Scope boundaries in §1.4 list rationale for every Out-of-Scope item.
- [ ] §11 names what's deliberately rejected, not just deferred.
- [ ] Two reviewers (one engineer, one designer/PM) have signed off in §0.
- [ ] At least one phase plan referencing this PRD exists in `docs/<slug>/phase-XX.md`.

## Maintenance Rules

- **Edit in place** for the active version. Bump PATCH on clarifications, MINOR on additions, MAJOR on breaking scope changes. Always update §0.
- **Append milestone deltas** in `Milestone-X.Y-<name>.md` rather than rewriting the parent PRD.
- **Phase plans never duplicate** the PRD -- they reference sections (`see PRD §7.1.3`).
- **Stale rejection.** If a "rejected" item from §11 becomes relevant, move it -- do not silently delete the rejection rationale; cross-reference it from the new PRD section.

## Anti-Patterns to Reject in Review

| Smell | Why it's bad | Fix |
|-|-|-|
| §6 lists fields without types or constraints. | Engineers will guess. | Type, required, max length, references -- all of it. |
| §1.3 uses "fast", "scalable", "intuitive". | Untestable. | Replace with measurable targets. |
| Workflows skip the offline branch. | Half the app is broken in airplane mode. | Add an "If offline:" branch to every step that touches the network. |
| Every section is "see code". | The PRD adds no value. | The PRD is the spec; code is the implementation. PRD comes first when there's conflict. |
| Module spec has no empty/error/loading states. | UI looks fine in dev, breaks on first user. | Add §X.Y "States" subsection per module. |
| §11 is empty. | Pretends the future doesn't exist. | At minimum, list 3-5 items in Horizon 1. |
| Section numbering is ad-hoc (mixes `§` and `1.`). | Hard to cross-reference. | Pick one and search-replace. |
| Schemas embedded as screenshots. | Can't be diffed or copy-pasted. | Always plain-text fenced code blocks. |
| Personas are generic ("the user"). | No empathy, no constraints surfaced. | Name, role, needs, an actual quote-style "use case". |
