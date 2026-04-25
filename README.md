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

To be filled in during Phase 1 once the skeleton exists.
