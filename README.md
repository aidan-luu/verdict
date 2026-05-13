# Verdict

A forecasting and risk-analysis workspace for FDA drug approval decisions.
For each upcoming PDUFA decision, the system surfaces structured context — a
historical reference class, market-implied probabilities, an optional LLM
analyst memo, and an optional decomposition tool — and helps a human
forecaster reason rigorously. The user's forecast is captured and, after
resolution, scored. Calibration is measured **comparatively** on the same
events for three sources of probability: the user, the LLM analyst, and the
prediction market.

The system does not predict outcomes. It surfaces context, captures human
judgment, and measures how well each source actually does over time.

## Why

Calibration ("when this forecaster says 70%, do events in that bucket happen
70% of the time?") is the right measure of forecasting quality, but it is
only meaningful when the forecaster has done real work on each question.
Verdict gives a forecaster the same desk an FDA analyst would want, then
measures how well each source of probability — including the user — actually
does over resolved events.

## Stack

- **Backend:** Rust (Axum, Tokio, SQLx)
- **Database:** Postgres 16
- **Background jobs:** Apalis + Redis
- **Frontend:** Next.js 15, TypeScript, Tailwind, Recharts
- **Auth:** Clerk
- **LLM ingestion:** Google Gemini API
- **Hosting:** local-first for now; Fly.io (backend) + Vercel (frontend) when deploying

## Status

Phase 1 (spine) shipped; Phase 2 (FDA briefing ingestion via Gemini) in flight;
Phase 3 (risk-analysis workspace pivot — reference class, LLM analyst memo,
decomposition, market prices, comparative calibration) planned in
[`docs/plans/phase-3.md`](docs/plans/phase-3.md). See `SPEC.md` for full scope
and phase definitions.

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
