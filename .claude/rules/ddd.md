---
paths:
  - "**/domains/**"
  - "**/domain/**"
  - "**/infrastructure/**"
  - "**/presentation/**"
  - "**/features/**"
---

# Domain-Driven Design Rules

DDD with Hexagonal Architecture, applied across all three surfaces:

| Surface | Domain layer lives in | Implementations live in | Boundary layer |
|-|-|-|-|
| Frontend (React) | `src/features/<domain>/domain/` (rare; usually pure logic in hooks) | `src/features/<domain>/api/` | `src/pages/` and `components/<domain>/` |
| Tauri (Rust) | `src-tauri/src/domains/<name>/domain/` | `src-tauri/src/domains/<name>/infrastructure/` | `src-tauri/src/domains/<name>/commands.rs` |
| Sync Server (Fastify) | `sync-server/src/app/domains/<name>/domain/` | `sync-server/src/app/domains/<name>/infrastructure/` | `sync-server/src/app/domains/<name>/presentation/` |

## Layer Rules

| Layer | Purpose | Allowed dependencies |
|-|-|-|
| **Domain** | Pure business logic, entities, value objects, services, repository interfaces (ports), domain events. | None. No `tauri`, no `sqlx`, no `prisma`, no `axios`, no `react`. Only the language standard library and small pure utilities. |
| **Presentation** | HTTP routes / Tauri commands / React components. Validates input, transforms HTTP <-> Domain, wires errors. | Domain. |
| **Infrastructure** | DB, external APIs, queues, sync transports. Implements the domain's repository interfaces. | Domain. |

**Key Principles:**
- Domain layer has **zero external dependencies** -- easily testable, reusable across surfaces if needed.
- Infrastructure implements domain interfaces (Dependency Inversion).
- Presentation transforms transport DTOs <-> Domain objects.
- Each domain is a **Bounded Context** with its own ubiquitous language.

## Entity Pattern

- An aggregate root has factory constructors:
  - `create()` (or `try_new()` in Rust) -- builds a brand-new entity, runs invariant checks.
  - `reconstitute()` (or `from_row()`) -- rebuilds from persisted state, no validation (the DB is trusted).
- Methods mutate the entity through intent-revealing names (`approve()`, `terminate()`, `markSynced()`), not setters.
- Serializers:
  - `toResponse()` / `to_response()` -- shapes the entity for outbound API/IPC. Nullable fields use `?? null` (TS) / `Option<T>` (Rust). Never `?.`.
  - `toPrisma()` / `to_row()` -- shapes the entity for persistence.
- Validation is centralized in the constructor; methods may add transition-specific checks.

## Repository Pattern

- **Interface** in `domain/repositories/` (TypeScript `interface`, Rust `trait`).
- **Implementation** in `infrastructure/repositories/` (Prisma class, sqlx struct).
- Repositories return domain entities, NOT raw rows or DTOs.
- Repository methods are intent-named (`findActiveById`, `lockForOffboarding`), not generic CRUD only.

## Adding a New Domain

### Tauri (Rust)
```bash
mkdir -p src-tauri/src/domains/<name>/{domain/{entities,services,repositories},infrastructure/repositories}
touch src-tauri/src/domains/<name>/{mod.rs,commands.rs}
```
Then:
1. Define the entity in `domain/entities/<name>.rs`.
2. Define the repository trait in `domain/repositories/<name>.rs`.
3. Implement the sqlx repository in `infrastructure/repositories/sqlx_<name>.rs`.
4. Define commands in `commands.rs` and re-export from `mod.rs`.
5. Register handlers in `lib.rs::run()` via `tauri::generate_handler!`.
6. Add a migration in `src-tauri/migrations/NNN_<name>.sql`.
7. If the entity syncs: add it to the sync engine's entity registry and declare its conflict policy.

### Sync Server (Fastify)
```bash
mkdir -p sync-server/src/app/domains/<name>/{domain/{entities,services,repositories},presentation/{routes,schemas},infrastructure/repositories}
```
Then:
1. Define the entity, repository interface, and Prisma repository.
2. Add the model to `prisma/schema.prisma`.
3. If the model has `entityId` -> add it to `TENANT_MODELS`.
4. Define TypeBox schemas in `presentation/schemas/`.
5. Define routes in `presentation/routes/<name>.routes.ts` with full Swagger.
6. The autoload picks up the domain's `index.ts`.
7. `docker compose restart sync-server` -- migration auto-applies on start.

### Frontend (React)
```
src/features/<name>/
├── api/                # ipc-typed wrappers, react-query hooks
├── components/         # forms, tables, dialogs (no shadcn; those go in components/ui)
├── hooks/              # business hooks (use<Action>, use<Resource>List)
├── schemas/            # Zod schemas for forms + IPC payloads
└── index.ts            # public surface re-exports
```

## Autoload Order (Sync Server)

1. **Plugins** from `src/app/plugins/` -- shared functionality (auth, prisma, redis, swagger).
2. **Domain modules** from `src/app/domains/` -- each domain's `index.ts` registers its routes.
3. **Sync routes** from `src/app/sync/routes/`.
4. **Global routes** from `src/app/routes/` -- health check, version, cross-domain endpoints.

## Layer Cross-Cutting Rules

- A domain entity NEVER imports from another domain. Cross-domain operations live in an application service or domain event.
- Domain events are emitted by entities (`onEvent('Hired', payload)`) and dispatched by the application layer. Sync server uses BullMQ; Tauri uses `tauri::Event` + an in-process bus.
- Value objects are immutable; equality is structural.
- `id` is part of the entity, not a separate value object, unless the project decides otherwise in a phase.
