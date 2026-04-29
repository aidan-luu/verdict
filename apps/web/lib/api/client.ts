import { getApiBaseUrl } from "../config";
import { type HealthResponse, healthResponseSchema } from "../validators";

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
