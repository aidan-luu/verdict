// Phase 3 PR B: reference-class panel for the event detail page.
// Renders aggregate stats first with explicit caveats around openFDA's
// approvals-only bias, then an expandable list of historical matches.
//
// Key UI rules from SPEC.md / phase-3.md / historical_events_curation.md:
// - The base rate is only displayed when the matched class has at least
//   5 approvals AND at least 5 CRLs. Otherwise we show qualitative
//   context with the absence reason.
// - The current event must have at least one controlled-vocab feature
//   for the matcher to return anything. If not, we show a prompt.

import type {
  BaseRateAbsenceReason,
  MatchReason,
  ReferenceClassAggregate,
  ReferenceClassHit,
  ReferenceClassResponse
} from "../lib/validators";

const MATCH_REASON_LABELS: Record<MatchReason, string> = {
  indication_area: "indication",
  application_type: "app type",
  primary_endpoint_type: "endpoint",
  advisory_committee_held: "AdCom held"
};

function formatPercent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

function formatSimilarity(value: number): string {
  return value.toFixed(2);
}

function describeAbsenceReason(reason: BaseRateAbsenceReason): string {
  switch (reason) {
    case "approval_only_bias":
      return "openFDA tracks approvals but not CRLs, so a base rate would be trivially close to 100%. Add manually-reviewed CRL rows to unlock a base rate for this class.";
    case "insufficient_sample":
      return "Need at least 5 approvals and 5 CRLs in this class before reporting a base rate. The matched pool below is still useful as qualitative context.";
  }
}

function outcomeLabel(outcome: string): string {
  switch (outcome) {
    case "approved":
      return "Approved";
    case "approved_with_rems":
      return "Approved (REMS)";
    case "crl":
      return "CRL";
    default:
      return outcome;
  }
}

function outcomePillClasses(outcome: string): string {
  if (outcome === "crl") {
    return "bg-red-100 text-red-800";
  }
  if (outcome === "approved" || outcome === "approved_with_rems") {
    return "bg-green-100 text-green-800";
  }
  return "bg-gray-100 text-gray-700";
}

export function ReferenceClassAggregateBlock({
  aggregate
}: {
  aggregate: ReferenceClassAggregate;
}) {
  return (
    <div className="rounded border p-4">
      <div className="flex flex-wrap items-baseline gap-x-6 gap-y-2">
        <div>
          <p className="text-xs uppercase tracking-wide text-gray-500">Sample size</p>
          <p className="text-2xl font-semibold">{aggregate.sample_size}</p>
        </div>
        <div>
          <p className="text-xs uppercase tracking-wide text-gray-500">Approved</p>
          <p className="text-2xl font-semibold text-green-700">{aggregate.approval_count}</p>
        </div>
        <div>
          <p className="text-xs uppercase tracking-wide text-gray-500">CRL</p>
          <p className="text-2xl font-semibold text-red-700">{aggregate.crl_count}</p>
        </div>
        <div>
          <p className="text-xs uppercase tracking-wide text-gray-500">Enrichment coverage</p>
          <p className="text-2xl font-semibold">{aggregate.enrichment_coverage_pct}%</p>
        </div>
      </div>

      <div className="mt-4">
        {aggregate.base_rate != null ? (
          <div>
            <p className="text-xs uppercase tracking-wide text-gray-500">Base rate (approval)</p>
            <p className="text-2xl font-semibold">{formatPercent(aggregate.base_rate)}</p>
            <p className="mt-1 text-xs text-gray-600">
              Treat this as one input, not authoritative. The class still inherits
              openFDA&apos;s coverage limits.
            </p>
          </div>
        ) : aggregate.base_rate_absence_reason ? (
          <div className="rounded bg-amber-50 p-3 text-sm text-amber-900">
            <p className="font-medium">No base rate displayed</p>
            <p className="mt-1">{describeAbsenceReason(aggregate.base_rate_absence_reason)}</p>
          </div>
        ) : null}
      </div>
    </div>
  );
}

function MatchChips({ reasons }: { reasons: MatchReason[] }) {
  return (
    <span className="flex flex-wrap gap-1">
      {reasons.map((reason) => (
        <span
          key={reason}
          className="rounded bg-blue-50 px-2 py-0.5 text-xs text-blue-800"
        >
          {MATCH_REASON_LABELS[reason]}
        </span>
      ))}
    </span>
  );
}

function ReferenceClassHitRow({ hit }: { hit: ReferenceClassHit }) {
  return (
    <li className="grid grid-cols-12 gap-2 border-t py-3 text-sm">
      <div className="col-span-12 sm:col-span-4">
        <p className="font-medium">{hit.drug_name}</p>
        <p className="text-xs text-gray-600">
          {hit.sponsor_name} &middot; {hit.application_number}
        </p>
      </div>
      <div className="col-span-6 sm:col-span-3">
        <p className="text-xs uppercase tracking-wide text-gray-500">Outcome</p>
        <p>
          <span
            className={`inline-block rounded px-2 py-0.5 text-xs font-medium ${outcomePillClasses(
              hit.decision_outcome
            )}`}
          >
            {outcomeLabel(hit.decision_outcome)}
          </span>
        </p>
        <p className="mt-1 text-xs text-gray-600">{hit.approval_date}</p>
      </div>
      <div className="col-span-6 sm:col-span-3">
        <p className="text-xs uppercase tracking-wide text-gray-500">Matched on</p>
        <MatchChips reasons={hit.match_reasons} />
      </div>
      <div className="col-span-12 sm:col-span-2 sm:text-right">
        <p className="text-xs uppercase tracking-wide text-gray-500">Similarity</p>
        <p className="font-mono">{formatSimilarity(hit.similarity_score)}</p>
      </div>
    </li>
  );
}

export function ReferenceClassPanel({ data }: { data: ReferenceClassResponse }) {
  const { aggregate, matches, current_features } = data;

  if (!current_features.has_any_feature) {
    return (
      <section className="rounded border p-4">
        <h2 className="text-lg font-semibold">Reference class</h2>
        <p className="mt-2 text-sm text-gray-700">
          This event has no controlled-vocabulary features yet (indication area,
          application type, endpoint type, AdCom held), so we can&apos;t build a
          reference class. Populate at least one feature on the event to enable
          matching.
        </p>
      </section>
    );
  }

  if (matches.length === 0) {
    return (
      <section className="rounded border p-4">
        <h2 className="text-lg font-semibold">Reference class</h2>
        <p className="mt-2 text-sm text-gray-700">
          No enriched historical events match this event&apos;s features. Either
          the enriched dataset is too small for this class, or none of the
          historical features overlap.
        </p>
      </section>
    );
  }

  if (matches.length < 5) {
    return (
      <section className="space-y-4">
        <h2 className="text-lg font-semibold">Reference class</h2>
        <p className="rounded bg-amber-50 p-3 text-sm text-amber-900">
          Only {matches.length} match{matches.length === 1 ? "" : "es"} found.
          Insufficient reference class for this event type — treat the matches
          below as anecdotes, not a class.
        </p>
        <ul className="rounded border p-2">
          {matches.map((hit) => (
            <ReferenceClassHitRow key={hit.historical_event_id} hit={hit} />
          ))}
        </ul>
      </section>
    );
  }

  return (
    <section className="space-y-4">
      <h2 className="text-lg font-semibold">Reference class</h2>
      <ReferenceClassAggregateBlock aggregate={aggregate} />
      <details className="rounded border p-4">
        <summary className="cursor-pointer text-sm font-medium">
          Show {matches.length} historical match{matches.length === 1 ? "" : "es"}
        </summary>
        <ul className="mt-2">
          {matches.map((hit) => (
            <ReferenceClassHitRow key={hit.historical_event_id} hit={hit} />
          ))}
        </ul>
      </details>
    </section>
  );
}
