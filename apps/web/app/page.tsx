import { ApiError, createForecast, fetchEvents, fetchHealth } from "../lib/api/client";
import { createForecastInputSchema } from "../lib/validators";
import { ForecastForm, type ForecastFormState } from "./forecast-form";
import Link from "next/link";

export default async function HomePage() {
  try {
    const health = await fetchHealth();
    const events = await fetchEvents("upcoming");

    async function submitForecastAction(
      _state: ForecastFormState,
      formData: FormData
    ): Promise<ForecastFormState> {
      "use server";

      const parsed = createForecastInputSchema.safeParse({
        eventId: String(formData.get("eventId") ?? ""),
        probability: Number(formData.get("probability")),
        rationale: String(formData.get("rationale") ?? "")
      });

      if (!parsed.success) {
        return {
          error: parsed.error.issues[0]?.message ?? "Invalid forecast submission",
          success: null
        };
      }

      try {
        await createForecast(parsed.data);
        return {
          error: null,
          success: "Forecast created successfully."
        };
      } catch (error) {
        if (error instanceof ApiError && error.status === 404) {
          return {
            error: "The selected event no longer exists.",
            success: null
          };
        }

        if (error instanceof ApiError && error.status === 409) {
          return {
            error: "That event is no longer upcoming, so a forecast cannot be added.",
            success: null
          };
        }

        return {
          error: "Could not save forecast. Please try again.",
          success: null
        };
      }
    }

    return (
      <main className="max-w-3xl mx-auto p-6">
        <h1 className="text-2xl font-semibold">Verdict</h1>
        <p className="mt-2">Backend status: {health.status}</p>
        <section className="mt-6 rounded border p-4">
          <h2 className="text-lg font-semibold">Upcoming events</h2>
          {events.length === 0 ? (
            <p className="mt-2 text-sm">No upcoming events available.</p>
          ) : (
            <ul className="mt-3 list-disc space-y-1 pl-5">
              {events.map((event) => (
                <li key={event.id}>
                  <span className="font-medium">{event.title}</span>{" "}
                  <span className="text-sm text-gray-600">({event.decision_date})</span>
                </li>
              ))}
            </ul>
          )}
        </section>
        {events.length > 0 ? (
          <ForecastForm events={events} submitForecastAction={submitForecastAction} />
        ) : null}
        <p className="mt-6 flex flex-wrap gap-x-4 gap-y-2">
          <Link className="text-blue-700 underline" href="/ingest">
            Ingest FDA briefing (PDF URL)
          </Link>
          <Link className="text-blue-700 underline" href="/calibration">
            Open calibration dashboard
          </Link>
        </p>
      </main>
    );
  } catch {
    return (
      <main className="max-w-3xl mx-auto p-6">
        <h1 className="text-2xl font-semibold">Verdict</h1>
        <p className="mt-2">Backend status: unavailable</p>
      </main>
    );
  }
}
