import { z } from "zod";

export const healthResponseSchema = z.object({
  status: z.literal("ok")
});

export type HealthResponse = z.infer<typeof healthResponseSchema>;

export const eventSchema = z.object({
  id: z.uuid(),
  title: z.string(),
  kind: z.string(),
  drug_name: z.string(),
  sponsor: z.string(),
  indication: z.string(),
  decision_date: z.string(),
  status: z.enum(["upcoming", "resolved", "voided"]),
  advisory_committee_date: z.string().nullable().optional(),
  primary_endpoint: z.string().nullable().optional(),
  advisory_committee_vote: z.string().nullable().optional(),
  source_url: z.string().nullable().optional(),
  // Phase 3 PR B: controlled-vocab features used by the reference-class matcher.
  indication_area: z.string().nullable().optional(),
  application_type: z.string().nullable().optional(),
  primary_endpoint_type: z.string().nullable().optional(),
  advisory_committee_held: z.boolean().nullable().optional()
});

export const eventListSchema = z.array(eventSchema);
export type Event = z.infer<typeof eventSchema>;

// Phase 3 PR B: reference-class panel.
export const matchReasonSchema = z.enum([
  "indication_area",
  "application_type",
  "primary_endpoint_type",
  "advisory_committee_held"
]);
export type MatchReason = z.infer<typeof matchReasonSchema>;

export const baseRateAbsenceReasonSchema = z.enum([
  "approval_only_bias",
  "insufficient_sample"
]);
export type BaseRateAbsenceReason = z.infer<typeof baseRateAbsenceReasonSchema>;

export const referenceClassHitSchema = z.object({
  historical_event_id: z.uuid(),
  application_number: z.string(),
  drug_name: z.string(),
  sponsor_name: z.string(),
  application_type: z.string(),
  approval_date: z.string(),
  indication_area: z.string().nullable().optional(),
  primary_endpoint_type: z.string().nullable().optional(),
  advisory_committee_held: z.boolean().nullable().optional(),
  advisory_committee_vote: z.string().nullable().optional(),
  decision_outcome: z.string(),
  enrichment_status: z.string(),
  similarity_score: z.number(),
  match_reasons: z.array(matchReasonSchema)
});
export type ReferenceClassHit = z.infer<typeof referenceClassHitSchema>;

export const referenceClassAggregateSchema = z.object({
  sample_size: z.number().int().nonnegative(),
  approval_count: z.number().int().nonnegative(),
  crl_count: z.number().int().nonnegative(),
  base_rate: z.number().nullable().optional(),
  base_rate_absence_reason: baseRateAbsenceReasonSchema.nullable().optional(),
  enrichment_coverage_pct: z.number().int().min(0).max(100)
});
export type ReferenceClassAggregate = z.infer<typeof referenceClassAggregateSchema>;

export const currentEventFeaturesSchema = z.object({
  indication_area: z.string().nullable().optional(),
  application_type: z.string().nullable().optional(),
  primary_endpoint_type: z.string().nullable().optional(),
  advisory_committee_held: z.boolean().nullable().optional(),
  has_any_feature: z.boolean()
});
export type CurrentEventFeatures = z.infer<typeof currentEventFeaturesSchema>;

export const referenceClassResponseSchema = z.object({
  event_id: z.uuid(),
  current_features: currentEventFeaturesSchema,
  matches: z.array(referenceClassHitSchema),
  aggregate: referenceClassAggregateSchema
});
export type ReferenceClassResponse = z.infer<typeof referenceClassResponseSchema>;

export const createForecastInputSchema = z.object({
  eventId: z.uuid(),
  probability: z
    .number()
    .min(0, "Probability must be between 0 and 1")
    .max(1, "Probability must be between 0 and 1"),
  rationale: z.string().min(1, "Rationale is required")
});
export type CreateForecastInput = z.infer<typeof createForecastInputSchema>;

/** Form + server action: paste an FDA briefing PDF URL (Phase 2 ingest). */
export const ingestFdaBriefingInputSchema = z.object({
  pdfUrl: z
    .string()
    .trim()
    .min(1, "PDF URL is required")
    .url("Enter a valid URL")
    .refine(
      (value: string) =>
        value.startsWith("https:") ||
        value.startsWith("http://127.0.0.1") ||
        value.startsWith("http://localhost"),
      "Use HTTPS, or http://localhost for local dev stubs"
    )
});
export type IngestFdaBriefingInput = z.infer<typeof ingestFdaBriefingInputSchema>;

export const forecastSchema = z.object({
  id: z.uuid(),
  user_id: z.uuid(),
  event_id: z.uuid(),
  probability: z.string(),
  rationale: z.string()
});

export type Forecast = z.infer<typeof forecastSchema>;

export const scoreContributionSchema = z.object({
  forecast_id: z.uuid(),
  event_id: z.uuid(),
  probability: z.string(),
  occurred: z.boolean(),
  brier_contribution: z.string()
});

export const scoreSummarySchema = z.object({
  resolved_forecast_count: z.number(),
  total_brier: z.string(),
  mean_brier: z.string(),
  contributions: z.array(scoreContributionSchema)
});

export type ScoreContribution = z.infer<typeof scoreContributionSchema>;
export type ScoreSummary = z.infer<typeof scoreSummarySchema>;
