# Verdict

A forecasting journal for FDA drug approval decisions. The user logs a
probability estimate on an upcoming PDUFA decision, the system tracks the
forecast through resolution, and surfaces calibration metrics (Brier score,
reliability diagram) along with comparison against market-implied probabilities
from Polymarket and Kalshi.

## Why

Calibration is the answer to one question: when a forecaster says 70%, do
events in that bucket actually happen 70% of the time? Verdict measures that
across a narrow domain (FDA decisions) and benchmarks the answer against
prediction markets on the same events.

## Stack

- **Backend:** Rust (Axum, Tokio, SQLx)
- **Database:** Postgres 16
- **Background jobs:** Apalis + Redis
- **Frontend:** Next.js 15, TypeScript, Tailwind, Recharts
- **Auth:** Clerk
- **LLM ingestion:** Anthropic API
- **Hosting:** Fly.io (backend) + Vercel (frontend)

## Status

Phase 1 — spine. See `SPEC.md` for full scope and phase definitions.

## Repository contents

- `SPEC.md` — what is being built and what is explicitly out of scope.
- `CLAUDE.md` — agent operating instructions.
- `.cursor/rules/verdict.mdc` — Cursor rules (short version of CLAUDE.md).
- `apps/` — added in Phase 1, will hold `api/` (Rust workspace) and
  `web/` (Next.js).
- `migrations/` — SQLx migrations.

## Local development

1. Start Postgres:

```bash
docker compose up -d
```

2. Prepare backend env and run API:

```bash
cp apps/api/.env.example apps/api/.env
cd apps/api
DATABASE_URL=postgres://verdict:verdict@127.0.0.1:5432/verdict sqlx migrate run
DATABASE_URL=postgres://verdict:verdict@127.0.0.1:5432/verdict cargo run
```

3. Prepare frontend env and run web app:

```bash
cp apps/web/.env.example apps/web/.env.local
cd apps/web
pnpm dev
```

The frontend runs on `http://localhost:3001` and calls the API on `http://127.0.0.1:3000`.

## Seed data

Phase 1 demo data is provided in `apps/api/seeds/phase1_demo.sql`.
Run it manually after migrations so tests remain deterministic:

```bash
docker compose exec -T postgres psql -U verdict -d verdict < apps/api/seeds/phase1_demo.sql
```

The seed inserts ~10 FDA-like events (resolved + upcoming) and resolved forecasts so
the calibration dashboard is populated on day one.

## Deploy notes (Phase 1)

- Backend deploy target: Fly.io (`fly.toml` at repo root).
- Frontend deploy target: Vercel (`apps/web`), set `NEXT_PUBLIC_API_BASE_URL`.
- Ensure production `DATABASE_URL` is set for backend migrations/startup.
