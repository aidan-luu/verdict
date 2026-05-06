import { getApiBaseUrl } from "../config";
import {
  createForecastInputSchema,
  type CreateForecastInput,
  eventListSchema,
  forecastSchema,
  type HealthResponse,
  healthResponseSchema,
  type Event,
  type Forecast,
  scoreSummarySchema,
  type ScoreSummary
} from "../validators";

export class ApiError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }
}

export async function fetchHealth(): Promise<HealthResponse> {
  const response = await fetch(`${getApiBaseUrl()}/health`, {
    method: "GET",
    headers: {
      Accept: "application/json"
    },
    cache: "no-store"
  });

  if (!response.ok) {
    throw new Error(`health endpoint failed: ${response.status}`);
  }

  const json = await response.json();
  return healthResponseSchema.parse(json);
}

export async function fetchEvents(status = "upcoming"): Promise<Event[]> {
  const response = await fetch(`${getApiBaseUrl()}/events?status=${status}`, {
    method: "GET",
    headers: {
      Accept: "application/json"
    },
    cache: "no-store"
  });

  if (!response.ok) {
    throw new ApiError(response.status, `events endpoint failed: ${response.status}`);
  }

  const json = await response.json();
  return eventListSchema.parse(json);
}

export async function createForecast(input: CreateForecastInput): Promise<Forecast> {
  const parsed = createForecastInputSchema.parse(input);
  const response = await fetch(`${getApiBaseUrl()}/events/${parsed.eventId}/forecasts`, {
    method: "POST",
    headers: {
      Accept: "application/json",
      "Content-Type": "application/json"
    },
    cache: "no-store",
    body: JSON.stringify({
      probability: parsed.probability.toFixed(4),
      rationale: parsed.rationale
    })
  });

  if (!response.ok) {
    throw new ApiError(response.status, `forecast endpoint failed: ${response.status}`);
  }

  const json = await response.json();
  return forecastSchema.parse(json);
}

export async function fetchScoreSummary(): Promise<ScoreSummary> {
  const response = await fetch(`${getApiBaseUrl()}/forecasts/scores/summary`, {
    method: "GET",
    headers: {
      Accept: "application/json"
    },
    cache: "no-store"
  });

  if (!response.ok) {
    throw new ApiError(response.status, `score summary endpoint failed: ${response.status}`);
  }

  const json = await response.json();
  return scoreSummarySchema.parse(json);
}
