import Link from "next/link";
import { notFound } from "next/navigation";

import { ApiError, fetchEvent, fetchReferenceClass } from "../../../lib/api/client";
import { ReferenceClassPanel } from "../../../components/reference-class-panel";

interface EventDetailPageProps {
  params: Promise<{ id: string }>;
}

export default async function EventDetailPage({ params }: EventDetailPageProps) {
  const { id } = await params;

  let event;
  try {
    event = await fetchEvent(id);
  } catch (error) {
    if (error instanceof ApiError && error.status === 404) {
      notFound();
    }
    throw error;
  }

  // Reference-class lookup is best-effort. If the backend errors out
  // (e.g. database is being migrated), the rest of the detail page
  // still renders.
  let referenceClassData = null;
  let referenceClassError: string | null = null;
  try {
    referenceClassData = await fetchReferenceClass(id);
  } catch (error) {
    referenceClassError =
      error instanceof Error ? error.message : "Reference class lookup failed.";
  }

  return (
    <main className="mx-auto max-w-4xl space-y-6 p-6">
      <p className="text-sm">
        <Link className="text-blue-700 underline" href="/">
          &larr; Back to events
        </Link>
      </p>

      <header className="space-y-2">
        <h1 className="text-2xl font-semibold">{event.title}</h1>
        <p className="text-sm text-gray-600">
          {event.drug_name} &middot; {event.sponsor} &middot; PDUFA{" "}
          {event.decision_date}
        </p>
        <p className="text-sm">
          <span className="font-medium">Indication:</span> {event.indication}
        </p>
        <FeaturesRow event={event} />
      </header>

      {referenceClassError ? (
        <section className="rounded border border-red-200 bg-red-50 p-4 text-sm text-red-900">
          <p className="font-medium">Reference class unavailable</p>
          <p className="mt-1">{referenceClassError}</p>
        </section>
      ) : referenceClassData ? (
        <ReferenceClassPanel data={referenceClassData} />
      ) : null}
    </main>
  );
}

function FeaturesRow({
  event
}: {
  event: Awaited<ReturnType<typeof fetchEvent>>;
}) {
  const chips: { label: string; value: string }[] = [];
  if (event.indication_area) {
    chips.push({ label: "indication area", value: event.indication_area });
  }
  if (event.application_type) {
    chips.push({ label: "app type", value: event.application_type });
  }
  if (event.primary_endpoint_type) {
    chips.push({ label: "endpoint type", value: event.primary_endpoint_type });
  }
  if (event.advisory_committee_held != null) {
    chips.push({
      label: "AdCom",
      value: event.advisory_committee_held ? "held" : "not held"
    });
  }

  if (chips.length === 0) {
    return (
      <p className="text-xs text-gray-500">
        No reference-class features set on this event yet.
      </p>
    );
  }

  return (
    <p className="flex flex-wrap gap-2 text-xs">
      {chips.map((chip) => (
        <span
          key={chip.label}
          className="rounded bg-gray-100 px-2 py-0.5 font-mono text-gray-700"
        >
          {chip.label}: {chip.value}
        </span>
      ))}
    </p>
  );
}
