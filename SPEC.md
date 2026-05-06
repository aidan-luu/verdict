# Verdict — Specification

## What this is

Verdict is a forecasting journal for binary events, narrowly focused on FDA
drug approval decisions. The user logs a probability estimate on a future
PDUFA decision, the system tracks it through resolution, and surfaces
calibration metrics (Brier score, reliability diagram) along with comparison
against market-implied probabilities from Polymarket and Kalshi.

The single most important sentence: **Verdict closes the loop from estimate
to resolution to scoring.** Everything else exists to serve that loop.

## Why this exists

Calibration is the answer to a single question: when a forecaster says 70%,
do events in that bucket actually happen 70% of the time? Most people who
think they reason probabilistically have never measured whether they actually
do. Verdict measures that — across a narrow, tractable domain (FDA decisions)
— and benchmarks the result against prediction markets on the same events.

The narrow domain is deliberate. Becoming better-calibrated requires volume
and feedback, and feedback requires resolution within a reasonable window.
FDA PDUFA decisions resolve on published dates, have clean binary outcomes,
and have public source documents to forecast from. That makes them the right
substrate for a forecasting tool that has to actually close its loop.

## What this is NOT

- Not a multi-tenant app. Single user. No households, no sharing, no roles.
- Not a medical or financial advice tool. Forecasts are a personal judgment
  exercise, nothing more.
- Not a general-purpose event tracker. FDA PDUFA decisions only in v1.
  SCOTUS, M&A, congressional votes, and sports are explicitly out of scope.
- Not an "AI predicts probabilities for you" app. The user does the
  forecasting. The system measures and contextualizes.

## Core user loop (in priority order)

1. **Capture a forecast.** Pick an upcoming FDA decision, enter a probability
   (0–1), enter a free-text rationale, log it.
2. **Resolve a forecast.** When the decision happens, mark the event yes or
   no (or "voided" for the rare ambiguous case).
3. **Score it.** Compute Brier contribution per resolved forecast.
4. **Show calibration.** Reliability diagram across resolved forecasts;
   running Brier score; per-decade-of-probability hit rate.
5. **Compare to market.** Where Polymarket or Kalshi list a contract on the
   same event, show market-implied probability and the user's edge.

Items 1–4 are Phase 1. Item 5 is Phase 3. Anything else is decoration.

## Phases

Each phase is shippable on its own. Do not bundle work from a later phase
into an earlier one.

### Phase 1 — Spine
- Rust + Axum backend, Postgres + SQLx, Next.js + TypeScript frontend.
- Auth via Clerk (JWT verification middleware on the backend).
- During early Phase 1 slices, auth may be temporarily stubbed and is replaced by Clerk JWT middleware later in Phase 1.
- Manual event creation: title, drug name, sponsor, indication, decision date.
- Manual forecast creation: probability, rationale, link to event.
- Manual resolution: mark event yes / no / voided.
- Calibration dashboard: reliability diagram and running Brier score.
- Seed script populating ~10 historical events with known outcomes so the
  dashboard has something to render from day one.
- Deployed: Rust on Fly.io, Next.js on Vercel.

**Phase 1 is done when the loop works end-to-end on a live URL.**

### Phase 2 — LLM ingestion
- Pipeline: take an FDA briefing PDF (URL or upload), call the Anthropic API,
  extract a structured `Event` record (drug, sponsor, indication, advisory
  committee date, decision date, primary endpoint, advisory committee vote
  if held).
- Validation on the LLM's JSON output before insert. Reject and retry on
  schema failure rather than swallowing bad data.
- Verified working on at least three real documents from FDA.gov.

**Phase 2 is done when an FDA briefing URL produces a clean DB record.**

### Phase 3 — Market comparison
- Background job (Apalis + Redis) polls Polymarket and Kalshi APIs hourly
  for contracts matching tracked events.
- Store time-series of market-implied probability per event.
- Frontend: a user-vs-market reliability diagram, plus a per-event chart
  showing market probability over time with the user's forecast overlaid.

**Phase 3 is done when at least 5 events show user-vs-market comparison.**

### Phase 4 — Polish (optional)
- README polished for outside readers.
- Sentry on backend, Playwright covering the critical path.
- Brier and reliability calculations have unit tests with hand-computed
  expected values.

## Data model

```
User (id, clerk_id, created_at)

Event (
  id, title, kind, drug_name, sponsor, indication,
  decision_date, advisory_committee_date,
  primary_endpoint, status, source_url,
  created_at, resolved_at, outcome
)
  title: short human-readable label for lists and UI (e.g. drug + PDUFA date).
  kind: machine-readable event subtype; v1 is FDA PDUFA only, stored as `fda_pdufa`.
  status:  'upcoming' | 'resolved' | 'voided'
  outcome: 'approved' | 'rejected' | NULL

Forecast (
  id, user_id, event_id, probability, rationale, created_at
)
  probability: numeric(5,4) in [0, 1]

MarketPrice (id, event_id, source, probability, fetched_at)
  source: 'polymarket' | 'kalshi'
```

Indices: `event(decision_date)`, `forecast(user_id, event_id)`,
`market_price(event_id, fetched_at)`.

## Out of scope (do not build, do not refactor toward)

- i18n, analytics, A/B testing, feature flags.
- Email, SMS, push notifications. The user checks the dashboard when they
  check it; alerting is not in scope.
- Multi-tenant. No households, no sharing, no roles.
- Real-time anything. Polling is fine. WebSockets are not in scope.
- Mobile app. Mobile-responsive web is enough.
- Other event categories. SCOTUS, M&A, congress, sports — all out.
- "AI suggests probabilities." The user forecasts. The LLM ingests documents.
- Native PDF rendering. Link out to FDA.gov for the source.

## Definition of done

A task is done when:
- Tests pass, lint clean, typecheck clean.
- Deployed (for backend or frontend changes that affect runtime behavior).
- The behavior can be exercised end-to-end on a live URL.

A phase is done when its success criterion above is met. Not before, not
after. Resist scope creep into the next phase.
