# Verdict — Specification

## What this is

Verdict is a **forecasting and risk-analysis workspace for FDA drug approval
decisions**. For each upcoming PDUFA decision, the system surfaces structured
context — a historical reference class, market-implied probabilities, an
optional LLM analyst memo, and an optional decomposition tool — and helps a
human forecaster reason rigorously through the question. The user's final
probability is captured and, after resolution, scored.

The system does not predict outcomes. It surfaces context and captures human
judgment. Calibration is measured **comparatively** on the same events for
three sources of probability: the user, the LLM analyst, and the prediction
market. The honest output of the project is that comparative measurement, not
a claim that any single source is well-calibrated.

The single most important sentence: **Verdict gives a forecaster the same
desk an FDA analyst would want, then measures how well each source of
probability — including the user — actually does over resolved events.**

## Why this exists

Calibration questions ("when this forecaster says 70%, do events in that
bucket happen 70% of the time?") are the right way to measure forecasting
quality, but they only become meaningful when the forecaster has done real
work on each question. Asking someone for a probability with no context
produces noise. Asking after they have reviewed a reference class, the
market, an analyst memo, and optionally decomposed the question produces a
forecast worth measuring.

FDA drug approval decisions are the right substrate for this workspace.
They are information-dense (briefing documents, label history, advisory
committee transcripts), publicly resolved on published dates, binary in
practice, and they generate prediction-market contracts that can be compared
against. The narrow domain is deliberate: a workspace useful for FDA
decisions is achievable; a workspace useful for "general forecasting" is not.

## What this is NOT

- Not a multi-tenant app. Single user. No households, no sharing, no roles.
- Not a medical or financial advice tool. Forecasts are a personal judgment
  exercise, nothing more.
- Not a general-purpose event tracker. FDA drug approval decisions only.
  SCOTUS, M&A, congressional votes, and sports are explicitly out of scope.
- **Not an authoritative forecaster.** The system never produces "the
  probability" of an event on behalf of the user. LLM estimates, market
  prices, and reference-class base rates are presented as inputs to the
  user's reasoning, never as a substitute for it.

## Core user loop (analyst's desk, in order)

1. **Pick or ingest an event.** Either pick an existing upcoming event or
   ingest one from an FDA briefing PDF.
2. **Review the reference class.** Inspect historical FDA decisions similar
   to this one along the available features (indication area, application
   type, endpoint type, advisory committee status). Because openFDA tracks
   only approvals, this panel is **primarily qualitative context** — a set
   of similar past decisions and their features. Base rates are shown only
   when the matched class includes enough approvals **and** CRLs (rejection
   letters) to be meaningful.
3. **Review the market.** Look at Polymarket and Kalshi implied
   probabilities for a contract on this decision, with the recent
   time-series, if a contract exists.
4. **Optionally request the LLM analyst view.** Generate a structured memo
   (efficacy, safety, regulatory precedent, key risks, advisory-committee
   posture, point estimate with range) from the briefing document via
   Gemini. The memo is one input, explicitly labeled non-authoritative.
5. **Optionally decompose the forecast.** Break the question into
   conditional steps (e.g. AdCom favorable * given favorable, FDA approves)
   and let the product be computed live. Compare gut and decomposed
   probabilities; the system soft-prompts when they disagree by more than
   10 percentage points.
6. **Capture the forecast.** Enter a probability and rationale; the system
   records the context surfaces the user reviewed.
7. **Resolve and score.** On the published decision date, mark the event
   approved / rejected / voided. The user, LLM, and market probabilities on
   that event are then folded into their respective calibration series.
8. **Compare calibration.** Reliability diagram with three overlaid lines
   plus y = x. Brier scores with 95% bootstrap CIs and sample sizes per
   source.

## Phases

Each phase is shippable on its own. Do not bundle work from a later phase
into an earlier one.

### Phase 1 — Spine (shipped)

- Rust + Axum backend, Postgres + SQLx, Next.js + TypeScript frontend.
- Auth via Clerk (JWT verification middleware on the backend), with a
  single-user stub in early Phase 1 slices.
- Manual event creation: title, drug name, sponsor, indication, decision date.
- Manual forecast creation: probability, rationale, link to event.
- Manual resolution: mark event yes / no / voided.
- Calibration dashboard: reliability diagram and running Brier score for a
  single (user) cohort.
- Seed script populating ~10 historical events with known outcomes.

Phase 1's single-cohort calibration math (Brier, reliability buckets) is
correct under the pivot; it is reused per cohort in Phase 3.

### Phase 2 — LLM ingestion of briefings (shipped / in flight)

- Pipeline: take an FDA briefing PDF (URL or upload), call Gemini, extract a
  structured `Event` record, validate the JSON before insert.
- Verified working on at least three real FDA.gov documents.

Phase 2's briefing extractor stays as the **metadata ingestion path**. The
**LLM analyst memo** is a different artifact and is introduced in Phase 3.

### Phase 3 — Risk-analysis workspace (this pivot)

Phase 3 is the pivot from journal to workspace. It introduces:

- A historical FDA decision dataset via openFDA, with optional LLM
  enrichment for indication area, endpoint type, and advisory committee
  posture, and a manual override path for adding CRL records.
- A reference-class matching service and panel, with explicit handling for
  approval-only classes and small samples.
- An LLM analyst memo service and panel that produces a structured view of
  efficacy, safety, regulatory precedent, advisory committee posture, key
  risks, and a probability estimate with a range.
- An optional forecast decomposition tool.
- Market-price ingestion and per-event display.
- A comparative calibration dashboard for user / LLM / market with bootstrap
  confidence intervals.

Detailed PR breakdown in [docs/plans/phase-3.md](docs/plans/phase-3.md).

**Phase 3 is done when**, on a resolved event, all three cohorts (user, LLM,
market) have a stored probability and the calibration dashboard renders the
overlaid reliability lines and Brier scores with CIs.

### Phase 4 — Polish (optional)

- README polished for outside readers.
- Sentry on backend, Playwright covering the critical path.
- Brier, reliability bucketing, and bootstrap CI calculations have unit
  tests with hand-computed expected values where possible.

## Data model

```
User (id, clerk_id, created_at)

Event (
  id, title, kind, drug_name, sponsor, indication,
  decision_date, advisory_committee_date,
  primary_endpoint, status, source_url,
  market_contract_overrides,   -- nullable jsonb: manual Polymarket/Kalshi IDs
  created_at, resolved_at, outcome
)
  title: short human-readable label for lists and UI.
  kind: machine-readable event subtype; v1 is FDA drug approval (`fda_pdufa`).
  status:  'upcoming' | 'resolved' | 'voided'
  outcome: 'approved' | 'rejected' | NULL

HistoricalEvent (
  id, application_number, drug_name, sponsor_name, application_type,
  approval_date, review_priority, indication_area, primary_endpoint_type,
  advisory_committee_held, advisory_committee_vote, decision_outcome,
  enrichment_status, source, raw_openfda_data, notes, created_at, updated_at
)
  application_type: 'NDA' | 'BLA' | 'ANDA' | 'other'  (derived from app_number prefix)
  decision_outcome: 'approved' | 'approved_with_rems' | 'crl'
                    (openFDA-sourced rows are 'approved'; 'crl' comes only via
                    manual override path)
  enrichment_status: 'structured_only' | 'llm_enriched' | 'manually_reviewed'

Forecast (
  id, user_id, event_id, probability, rationale,
  gut_probability, decomposed_probability, discrepancy_flag,
  reviewed_reference_class, reviewed_market, reviewed_llm_analyst, reviewed_decomposition,
  created_at
)
  probability:           numeric(5,4) in [0, 1]   -- the user's final number
  gut_probability:       nullable numeric(5,4)    -- entered before decomposition
  decomposed_probability:nullable numeric(5,4)    -- product of conditional steps
  discrepancy_flag:      boolean                  -- set when |gut - decomposed| > 0.10
  reviewed_*:            booleans recording which context panels the user
                         viewed before submitting (influence tracking)

ForecastDecomposition (
  id, forecast_id, step_order, question, conditional_probability, reasoning
)

LlmAnalystMemo (
  id, event_id, efficacy_assessment, safety_assessment,
  regulatory_precedent_assessment, advisory_committee_posture,
  key_risks (text[]), estimated_probability, estimated_probability_low,
  estimated_probability_high, reasoning, model_version, generated_at
)
  Multiple memos per event are allowed; regeneration creates a new row,
  old rows are kept for audit.

LlmForecast (
  id, event_id, memo_id, probability, source, created_at
)
  source: 'llm_analyst_memo'   -- parallel to a user Forecast for calibration
  Stores the point estimate from a memo as a calibration-eligible probability.
  One LlmForecast per event is used per cohort calibration run (the most
  recent memo's point estimate).

MarketPrice (id, event_id, source, probability, fetched_at)
  source: 'polymarket' | 'kalshi'
```

Indices: `event(decision_date)`, `forecast(user_id, event_id)`,
`market_price(event_id, fetched_at)`, `historical_event(application_number)`,
`historical_event(indication_area)`, `historical_event(enrichment_status)`,
`llm_analyst_memo(event_id, generated_at DESC)`.

## Reference class semantics (important)

openFDA's `drug/drugsfda` endpoint covers FDA-approved drug products. It
does **not** cover applications that received a Complete Response Letter
(CRL) and were never approved. Therefore the historical dataset built in
Phase 3 PR A is structurally biased toward approval outcomes.

The workspace handles this honestly:

- The reference-class panel is **primarily qualitative context** — "here are
  similar past decisions and their features" — not a base-rate calculator.
- A base rate (approval percentage) is shown **only** when the matched
  reference class includes at least 5 approvals **and** at least 5 CRLs.
  Otherwise the UI says approval bias prevents a meaningful base rate.
- CRL records enter the dataset only through the manual override path
  (PR A) or future external sources. Adding CRLs for a specific feature
  class is the explicit way to unlock base-rate computation for that class.

## Out of scope (do not build, do not refactor toward)

- i18n, analytics, A/B testing, feature flags.
- Email, SMS, push notifications.
- Multi-tenant. No households, no sharing, no roles.
- Real-time anything. Polling is fine. WebSockets are not in scope.
- Mobile app. Mobile-responsive web is enough.
- Other event categories. SCOTUS, M&A, congress, sports — all out.
- **Authoritative AI predictions.** LLM analyst memos and probability
  estimates are stored and measured, but never presented as "the answer."
  The user does the forecasting; the system surfaces context and measures
  calibration of every source over time.
- Native PDF rendering. Link out to FDA.gov for the source.

## Definition of done

A task is done when:
- Tests pass, lint clean, typecheck clean.
- Deployed (for backend or frontend changes that affect runtime behavior).
- The behavior can be exercised end-to-end on a live URL.

A phase is done when its success criterion above is met. Not before, not
after. Resist scope creep into the next phase.
