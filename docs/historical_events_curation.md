# Historical events — curation guide

Verdict's reference-class panel ([Phase 3 PR B](plans/phase-3.md)) is built
on the `historical_event` table. This guide explains how that table is
populated and curated: the two binaries (`ingest_historical`,
`enrich_historical`), the manual override endpoint, and the
**approvals-only bias** that comes with openFDA's coverage.

## You need an openFDA API key

Before running anything, sign up for a free API key:

<https://open.fda.gov/apis/authentication/>

The key is **required**, not optional. openFDA's unauthenticated limit is
**1,000 requests per IP per day**, which cannot complete a full ingestion
run. With an API key the limits are 240 requests/min and 120,000
requests/day per key — comfortable for this project.

Put the key in `apps/api/.env`:

```
OPENFDA_API_KEY=your-real-key-here
```

Both binaries refuse to start if `OPENFDA_API_KEY` is missing or still
set to the `.env.example` placeholder.

## How the pipeline is shaped

```
openFDA drug/drugsfda                          openFDA drug/label
        │                                             │
        ▼                                             ▼
  ingest_historical  ─────────────────────▶  enrich_historical
        │                                             │
        ▼                                             ▼
  historical_event                            historical_event
  (structured_only)                           (llm_enriched)

                          POST /admin/historical_events/{id}
                                          │
                                          ▼
                                 historical_event
                                 (manually_reviewed)
```

- **`ingest_historical`** pulls the universe of original NDA/BLA approvals
  from openFDA and writes one row per application. It is idempotent: the
  natural key is `application_number`, and re-runs refresh structured
  fields without touching enrichment.
- **`enrich_historical`** picks rows at `enrichment_status =
  'structured_only'` and asks Gemini to extract `indication_area`,
  `primary_endpoint_type`, and `advisory_committee_*` from the
  prescribing-information label. Each field gets a per-field confidence
  score; fields below the threshold (default 0.7) are discarded. Surviving
  fields are written, and the row is promoted to `'llm_enriched'`.
- **`POST /admin/historical_events/{id}`** is the manual-override path.
  It is the only way a `decision_outcome = 'crl'` enters the dataset.

## ORIG-only selection (important)

openFDA's Lucene-style search does not correlate nested predicates within
the same array element. A query like

```
submissions.submission_type:ORIG AND
submissions.submission_status:AP AND
submissions.submission_status_date:[20100101 TO 20261231]
```

can return a record whose original (ORIG) submission was old but whose
later supplement (SUPPL) matched the date filter. The ingest binary
handles this in application code: it scans the `submissions[]` array, picks
the submission with `submission_type == "ORIG"` **and**
`submission_status == "AP"`, and uses **its** `submission_status_date` as
`approval_date`. Records whose original approval falls outside the
configured window are skipped.

This is covered by unit tests in
`apps/api/crates/api/src/services/openfda.rs` (search for
`original_approval_selector_ignores_recent_supplements` and
`map_record_skips_when_orig_is_before_window`).

## Commands

### Initial ingestion

```
cargo run --bin ingest_historical -- --from-date 2010-01-01
```

Flags:
- `--from-date YYYY-MM-DD` (default `2010-01-01`)
- `--to-date YYYY-MM-DD` (default: today)
- `--page-limit N` (default 100; openFDA hard max 1000)

The binary logs per-page counts and a final summary with breakdowns:
inserted, updated, skipped-by-reason, errors.

### Incremental enrichment

Run in small batches so quality can be reviewed before scaling up:

```
cargo run --bin enrich_historical -- --batch-size 50
```

Flags:
- `--batch-size N` (default 50, max 500)
- `--from-year YYYY` (only enrich rows with `approval_date` year >= this)
- `--sponsor "ILIKE pattern"` (e.g. `"Eli%"` to focus on a specific
  sponsor)

The binary is resumable: it always picks rows currently at
`structured_only`, so killing and restarting just continues where it left
off.

### Manual override

For high-importance applications where LLM enrichment is unreliable, or
to add a CRL outcome that openFDA does not cover:

```
curl -X POST http://127.0.0.1:3000/admin/historical_events/<id> \
  -H "content-type: application/json" \
  -d '{
        "decision_outcome": "crl",
        "indication_area": "neurological",
        "notes": "Added from FDA press release dated 2024-..."
      }'
```

The endpoint accepts any subset of fields and flips
`enrichment_status` to `'manually_reviewed'`. Subsequent ingestion runs
will refresh the structured fields (drug name, sponsor, date) but will
not touch manually-reviewed fields.

## Realistic LLM enrichment coverage

Prescribing-information labels are inconsistent. Plan for:

- **`indication_area`: high coverage.** Labels almost always state an
  indication, and the Gemini prompt maps it to a fixed vocabulary.
- **`primary_endpoint_type`: moderate coverage.** Many labels summarize
  endpoints; others gesture at them. Expect ~50–70% of labels to yield a
  usable answer.
- **`advisory_committee_held` / `advisory_committee_vote`: low coverage.**
  Labels almost never mention AdCom meetings. Expect these fields to be
  null on most enriched rows. Use the manual override path for the
  high-importance applications where this matters.

The enrichment binary writes whatever passes per-field validation and
leaves the rest null; subsequent enrichment runs can be re-tried as the
prompt or vocabulary evolves.

## Approvals-only bias (must surface in any UI)

openFDA's `drug/drugsfda` endpoint covers only approved drug products.
Applications that received a Complete Response Letter (CRL) and were
never subsequently approved are **not** in the dataset.

Consequences:

- The reference class for any current event will be biased toward
  approval outcomes.
- A naive "approval rate across the reference class" computation will
  always be close to 100%.
- The Phase 3 reference-class panel deliberately gates base-rate
  computation on the matched class containing **at least 5 approvals
  *and* at least 5 CRLs**, and labels the panel as qualitative context
  otherwise.

The intended way to unlock base-rate computation for a specific feature
class is to manually add CRL records via the override endpoint. Good
candidates are recent contested or rejected applications with publicly
documented outcomes, e.g. Aduhelm-precursor rejections, Relyvrio's
post-approval rescindment, or any high-profile CRL covered in industry
press.

## Tests to look at

- `apps/api/crates/api/src/services/openfda.rs` — mapper unit tests, the
  ORIG-vs-recent-SUPPL regression test, and the enrichment validator
  tests.
- `apps/api/crates/api/src/services/historical_event_repo.rs` —
  integration tests for upsert idempotency, enrichment-non-clobber, and
  manual-override status transitions.
- `apps/api/crates/api/src/routes/admin.rs` — admin endpoint tests
  covering vocabulary validation, empty-payload rejection, and the
  not-found case.
