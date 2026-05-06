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
  status: z.enum(["upcoming", "resolved", "voided"])
});

export const eventListSchema = z.array(eventSchema);
export type Event = z.infer<typeof eventSchema>;

export const createForecastInputSchema = z.object({
  eventId: z.uuid(),
  probability: z
    .number()
    .min(0, "Probability must be between 0 and 1")
    .max(1, "Probability must be between 0 and 1"),
  rationale: z.string().min(1, "Rationale is required")
});
export type CreateForecastInput = z.infer<typeof createForecastInputSchema>;

export const forecastSchema = z.object({
  id: z.uuid(),
  user_id: z.uuid(),
  event_id: z.uuid(),
  probability: z.string(),
  rationale: z.string()
});

export type Forecast = z.infer<typeof forecastSchema>;
