"use client";

import { useState } from "react";

import { createForecastInputSchema, type Event } from "../lib/validators";

export type ForecastFormState = {
  error: string | null;
  success: string | null;
};

type ForecastFormProps = {
  events: Event[];
  submitForecastAction: (state: ForecastFormState, formData: FormData) => Promise<ForecastFormState>;
};

const INITIAL_STATE: ForecastFormState = {
  error: null,
  success: null
};

export function ForecastForm({ events, submitForecastAction }: ForecastFormProps) {
  const [state, setState] = useState<ForecastFormState>(INITIAL_STATE);

  async function handleSubmit(formData: FormData) {
    const probabilityRaw = Number(formData.get("probability"));
    const rationaleRaw = String(formData.get("rationale") ?? "");
    const eventIdRaw = String(formData.get("eventId") ?? "");

    const parsed = createForecastInputSchema.safeParse({
      eventId: eventIdRaw,
      probability: probabilityRaw,
      rationale: rationaleRaw
    });

    if (!parsed.success) {
      setState({
        error: parsed.error.issues[0]?.message ?? "Invalid forecast form data",
        success: null
      });
      return;
    }

    const nextState = await submitForecastAction(INITIAL_STATE, formData);
    setState(nextState);
  }

  return (
    <section className="mt-6 rounded border p-4">
      <h2 className="text-lg font-semibold">Add forecast</h2>
      <form
        action={async (formData) => {
          await handleSubmit(formData);
        }}
        className="mt-4 flex flex-col gap-3"
      >
        <label className="flex flex-col gap-1">
          <span className="text-sm">Event</span>
          <select name="eventId" className="rounded border p-2" required>
            {events.map((event) => (
              <option key={event.id} value={event.id}>
                {event.title}
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-sm">Probability (0 to 1)</span>
          <input
            name="probability"
            type="number"
            min="0"
            max="1"
            step="0.0001"
            className="rounded border p-2"
            required
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-sm">Rationale</span>
          <textarea name="rationale" className="min-h-24 rounded border p-2" required />
        </label>
        <button type="submit" className="w-fit rounded bg-black px-4 py-2 text-white">
          Save forecast
        </button>
      </form>
      {state.error ? <p className="mt-3 text-sm text-red-600">{state.error}</p> : null}
      {state.success ? <p className="mt-3 text-sm text-green-700">{state.success}</p> : null}
    </section>
  );
}
