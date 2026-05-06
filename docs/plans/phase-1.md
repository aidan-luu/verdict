# Phase 1 — PR breakdown

Phase 1 closes the loop from forecast capture through resolution, scoring, and a calibration dashboard on Postgres + Axum + Next.js (see `SPEC.md`). This document splits work into small mergeable PRs (target roughly 100–300 LOC each).

Shipped PRs (1–3) summarize what landed in-repo. Remaining PRs (4–9) are implementation sketches and sequencing notes—not binding line counts.

---

## PR 1 — Backend scaffold + `/health` (shipped)

**Goal:** Establish the Rust workspace and a minimal Axum server that proves build, lint, and tests work end-to-end.

**Backend changes:**
- Cargo workspace under `apps/api/` with an API crate.
- `GET /health` returning JSON `{ "status": "ok" }`.
- Structured errors (`thiserror`), tracing subscriber setup.
- Unit/integration test for `/health`.

**Frontend changes:** None.

**Success criterion:** `cargo check`, `cargo clippy -- -D warnings`, and `cargo test` pass locally and in CI assumptions.

**Rough LOC estimate:** ~140–220 lines.

---

## PR 2 — Next.js scaffold + backend health probe (shipped)

**Goal:** Add the Phase 1 frontend shell and verify wiring to the API early.

**Backend changes:** None.

**Frontend changes:**
- Next.js 15 App Router app under `apps/web/` with strict TypeScript and Tailwind.
- Typed fetch plus validation (e.g. Zod) against `GET /health`.
- Simple home page surfacing backend reachability.
- One Vitest test for the client/parser path.
- Dev server defaults to port `3001` so it does not collide with the API on `3000`.

**Success criterion:** `pnpm typecheck`, `pnpm lint`, and `pnpm test` pass; page renders healthy/unhealthy state against a running API.

**Rough LOC estimate:** ~180–280 lines.

---

## PR 3 — Events API slice + local Postgres (shipped)

**Goal:** Persist manually entered FDA PDUFA-style events and list/filter them—first slice of the forecasting spine.

**Backend changes:**
- SQLx + Postgres migrations: `events` table with UUID primary key, `title`, `kind` (default `fda_pdufa`), lifecycle fields aligned with `SPEC.md`, nullable `source_url`, `status` with CHECK constraint, index on `decision_date`.
- Axum `AppState` holding `PgPool`; routes use `State<AppState>` (not `Extension`).
- `POST /events`, `GET /events` with optional `?status=…`.
- Startup migration run with an explicit TODO to revisit production migration strategy in Phase 4 polish.
- `docker-compose.yml` at repo root for Postgres 16; `DATABASE_URL` documented in `apps/api/.env.example`.
- Committed `.sqlx/` offline query metadata for compile-time macros without a live DB.

**Frontend changes:** None.

**Success criterion:** With Postgres up, migrations applied, and `DATABASE_URL` set, create/list flows work; Rust gates pass with offline SQLx metadata available.

**Rough LOC estimate:** ~230–320 lines.

---

## PR 4 — Forecast capture (stub user, backend-only)

**Goal:** Persist a forecast for an existing event: probability in `[0, 1]` as `numeric(5,4)` / `rust_decimal::Decimal`, plus rationale; enforce “event must be upcoming” for new forecasts.

**Backend changes:**
- **Path B (single-user stub):** Migration creates `users` table and inserts exactly one stub row with a **known UUID** (fixed seed). Forecast handlers use that UUID via a **named constant** until Clerk lands.
- Migration adds `forecasts` table per `SPEC.md` (`user_id`, `event_id`, `probability`, `rationale`, `created_at`) and index on `(user_id, event_id)`.
- **Nested routing:** `POST /events/{event_id}/forecasts` (Axum nested router under `/events`).
- Validate JSON body with `validator`; use SQLx macros only for SQL.
- Return **404** if the event id does not exist; return **409 Conflict** if the event’s `status` is not `upcoming`.
- In the forecast handler, add an explicit `// TODO` comment that the stub user id is temporary and will be replaced when Clerk JWT middleware is added later in Phase 1.

**Frontend changes:** None (UI in PR 7).

**Success criterion:** For an `upcoming` event, `POST` creates exactly one forecast row with correct decimal precision and returns the stored record; non-upcoming events yield 409.

**Rough LOC estimate:** ~220–290 lines.

---

## PR 5 — Event resolution

**Goal:** Manually resolve an event so downstream scoring has ground truth (`approved` / `rejected`, or `voided` when ambiguous).

**Backend changes:**
- Align `events` columns with `SPEC.md` if any gaps remain (`outcome`, `resolved_at`, status transitions).
- Resolution endpoint (e.g. `PATCH /events/{event_id}` or `POST /events/{event_id}/resolve`) with validated payload.
- Enforce allowed transitions (e.g. resolve from `upcoming`; explicit void path).

**Frontend changes:** None.

**Success criterion:** Resolution updates stored fields deterministically; reads reflect `resolved`/`voided` and correct `outcome`.

**Rough LOC estimate:** ~180–240 lines.

---

## PR 6 — Scoring (Brier contribution)

**Goal:** Compute per-forecast Brier contribution for resolved, non-voided outcomes using `Decimal` throughout stored math (no `f64` for stored values).

**Backend changes:**
- Small `scoring` module with pure functions over `rust_decimal::Decimal`.
- Minimal read API surface needed by PR 8 (summary and/or per-event slice).
- Hand-computed unit tests for fixed fixtures (do not “fix” tests to match wrong math).

**Frontend changes:** None.

**Success criterion:** Unit tests match hand-computed expected values per project math rules.

**Rough LOC estimate:** ~240–320 lines (including tests).

---

## PR 7 — Frontend: events list + forecast form

**Goal:** Capture forecasts from the browser against the running API.

**Backend changes:**
- Small glue only if needed (e.g. CORS allowlist for local Next origin).

**Frontend changes:**
- Pages for listing events (`GET /events`) and submitting a forecast via PR 4 route.
- Zod validation for probability and rationale; clear error display for 404/409 paths.

**Success criterion:** User can pick an upcoming event and submit a forecast that persists (verified by refresh or a simple follow-up fetch).

**Rough LOC estimate:** ~260–340 lines.

---

## PR 8 — Frontend: calibration dashboard

**Goal:** Show calibration visuals and headline metrics for resolved history (reliability-style buckets + running Brier / decade buckets per Phase 1 intent).

**Backend changes:**
- Aggregation endpoints built from resolved events + forecasts, reusing PR 6 scoring helpers where possible.

**Frontend changes:**
- Dashboard route using Recharts; empty states when no resolved data exists.

**Success criterion:** With seeded or manually resolved data, charts and summary numbers render consistently with backend aggregates.

**Rough LOC estimate:** ~280–360 lines.

---

## PR 9 — Seed data + deploy

**Goal:** Meet Phase 1 “definition of done”: demo dataset (~10 historical events with known outcomes) plus deployed API (Fly.io) and web app (Vercel).

**Backend changes:**
- Repeatable seed mechanism (migration-appropriate SQL seed, script, or small binary—pick one and document).
- Fly deploy artifacts; align migration execution story with the Phase 4 migration-on-startup revisit note.

**Frontend changes:**
- Production env wiring for `NEXT_PUBLIC_API_BASE_URL` and deployment notes.

**Success criterion:** Documented path to run locally and live URLs demonstrating event → forecast → resolve → score → dashboard.

**Rough LOC estimate:** ~220–320 lines (split across infra, seed, and docs).

---

## Cross-cutting concerns

- **Auth timing:** PR 4 uses a stub user row by design; Clerk middleware replaces the constant path later in Phase 1—avoid mixing half-measures that duplicate `user_id` sources.
- **API surface churn:** Keep event and forecast JSON shapes stable before PR 7 to limit frontend rework.
- **Aggregate vs pure math:** Keep bucketing/aggregation SQL straightforward in PR 8 while PR 6 remains the correctness anchor for decimal scoring.
