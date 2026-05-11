"use client";

import { useState } from "react";

import { ingestFdaBriefingInputSchema, type Event } from "../lib/validators";

export type BriefingIngestFormState = {
  error: string | null;
  success: string | null;
  createdEvent: Event | null;
};

type BriefingIngestFormProps = {
  submitIngestAction: (
    state: BriefingIngestFormState,
    formData: FormData
  ) => Promise<BriefingIngestFormState>;
};

const INITIAL_STATE: BriefingIngestFormState = {
  error: null,
  success: null,
  createdEvent: null
};

export function BriefingIngestForm({ submitIngestAction }: BriefingIngestFormProps) {
  const [state, setState] = useState<BriefingIngestFormState>(INITIAL_STATE);

  async function handleSubmit(formData: FormData) {
    const pdfUrlRaw = String(formData.get("pdfUrl") ?? "").trim();

    const parsed = ingestFdaBriefingInputSchema.safeParse({ pdfUrl: pdfUrlRaw });

    if (!parsed.success) {
      setState({
        error: parsed.error.issues[0]?.message ?? "Invalid URL",
        success: null,
        createdEvent: null
      });
      return;
    }

    const nextState = await submitIngestAction(INITIAL_STATE, formData);
    setState(nextState);
  }

  return (
    <section className="rounded border p-4">
      <h2 className="text-lg font-semibold">Ingest from FDA briefing PDF</h2>
      <p className="mt-1 text-sm text-gray-600">
        Paste a public HTTPS URL to a briefing PDF (must match API host allowlist, typically{" "}
        <span className="font-mono text-xs">fda.gov</span>).
      </p>
      <form
        action={async (formData) => {
          await handleSubmit(formData);
        }}
        className="mt-4 flex flex-col gap-3"
      >
        <label className="flex flex-col gap-1">
          <span className="text-sm">PDF URL</span>
          <input
            name="pdfUrl"
            type="url"
            placeholder="https://www.fda.gov/..."
            className="rounded border p-2 font-mono text-sm"
            required
            autoComplete="off"
          />
        </label>
        <button type="submit" className="w-fit rounded bg-black px-4 py-2 text-white">
          Run ingest
        </button>
      </form>
      {state.error ? <p className="mt-3 text-sm text-red-600">{state.error}</p> : null}
      {state.success ? <p className="mt-3 text-sm text-green-700">{state.success}</p> : null}
      {state.createdEvent ? (
        <div className="mt-4 rounded border border-green-200 bg-green-50 p-3 text-sm">
          <p className="font-semibold text-green-900">Created event</p>
          <ul className="mt-2 list-inside list-disc space-y-1 text-green-900">
            <li>
              <span className="font-medium">Title:</span> {state.createdEvent.title}
            </li>
            <li>
              <span className="font-medium">Drug:</span> {state.createdEvent.drug_name}
            </li>
            <li>
              <span className="font-medium">Sponsor:</span> {state.createdEvent.sponsor}
            </li>
            <li>
              <span className="font-medium">Indication:</span> {state.createdEvent.indication}
            </li>
            <li>
              <span className="font-medium">PDUFA / decision date:</span>{" "}
              {state.createdEvent.decision_date}
            </li>
            {state.createdEvent.source_url ? (
              <li>
                <span className="font-medium">Source:</span>{" "}
                <span className="break-all font-mono text-xs">{state.createdEvent.source_url}</span>
              </li>
            ) : null}
            {state.createdEvent.advisory_committee_date ? (
              <li>
                <span className="font-medium">Advisory committee:</span>{" "}
                {state.createdEvent.advisory_committee_date}
              </li>
            ) : null}
            {state.createdEvent.primary_endpoint ? (
              <li>
                <span className="font-medium">Primary endpoint:</span>{" "}
                {state.createdEvent.primary_endpoint}
              </li>
            ) : null}
            {state.createdEvent.advisory_committee_vote ? (
              <li>
                <span className="font-medium">Advisory vote:</span>{" "}
                {state.createdEvent.advisory_committee_vote}
              </li>
            ) : null}
          </ul>
        </div>
      ) : null}
    </section>
  );
}
