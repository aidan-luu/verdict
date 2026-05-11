import Link from "next/link";

import { ApiError, ingestFromFdaBriefing } from "../../lib/api/client";
import { ingestFdaBriefingInputSchema } from "../../lib/validators";
import { BriefingIngestForm, type BriefingIngestFormState } from "../briefing-ingest-form";

export default async function IngestPage() {
  async function submitIngestAction(
    _state: BriefingIngestFormState,
    formData: FormData
  ): Promise<BriefingIngestFormState> {
    "use server";

    const parsed = ingestFdaBriefingInputSchema.safeParse({
      pdfUrl: String(formData.get("pdfUrl") ?? "").trim()
    });

    if (!parsed.success) {
      return {
        error: parsed.error.issues[0]?.message ?? "Invalid URL",
        success: null,
        createdEvent: null
      };
    }

    try {
      const event = await ingestFromFdaBriefing(parsed.data.pdfUrl);
      return {
        error: null,
        success: "Event created from briefing URL.",
        createdEvent: event
      };
    } catch (error) {
      if (error instanceof ApiError) {
        return {
          error: error.message,
          success: null,
          createdEvent: null
        };
      }

      return {
        error: "Could not reach the API or ingest failed. Check that the API is running.",
        success: null,
        createdEvent: null
      };
    }
  }

  return (
    <main className="mx-auto max-w-3xl p-6">
      <h1 className="text-2xl font-semibold">Briefing ingest</h1>
      <p className="mt-2 text-sm text-gray-700">
        Phase 2 operator path: URL → fetch PDF → Gemini → validate → new upcoming event.
      </p>
      <div className="mt-6">
        <BriefingIngestForm submitIngestAction={submitIngestAction} />
      </div>
      <p className="mt-6">
        <Link className="text-blue-700 underline" href="/">
          Back to home
        </Link>
      </p>
    </main>
  );
}
