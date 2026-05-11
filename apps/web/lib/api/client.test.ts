import { afterEach, describe, expect, it, vi } from "vitest";

import {
  createForecast,
  fetchEvents,
  fetchHealth,
  fetchScoreSummary,
  ingestFromFdaBriefing
} from "./client";

describe("fetchHealth", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.unstubAllGlobals();
  });

  it("returns parsed health payload", async () => {
    vi.stubEnv("NEXT_PUBLIC_API_BASE_URL", "http://example.test");
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: true,
        json: async () => ({ status: "ok" })
      }))
    );

    await expect(fetchHealth()).resolves.toEqual({ status: "ok" });
  });
});

describe("fetchEvents", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.unstubAllGlobals();
  });

  it("returns parsed events payload", async () => {
    vi.stubEnv("NEXT_PUBLIC_API_BASE_URL", "http://example.test");
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: true,
        json: async () => [
          {
            id: "11111111-1111-4111-8111-111111111111",
            title: "Drug A PDUFA",
            kind: "fda_pdufa",
            drug_name: "Drug A",
            sponsor: "Sponsor",
            indication: "Condition",
            decision_date: "2026-12-01",
            status: "upcoming"
          }
        ]
      }))
    );

    await expect(fetchEvents()).resolves.toHaveLength(1);
  });
});

describe("createForecast", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.unstubAllGlobals();
  });

  it("returns parsed forecast payload", async () => {
    vi.stubEnv("NEXT_PUBLIC_API_BASE_URL", "http://example.test");
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: true,
        json: async () => ({
          id: "22222222-2222-4222-8222-222222222222",
          user_id: "00000000-0000-4000-8000-000000000001",
          event_id: "11111111-1111-4111-8111-111111111111",
          probability: "0.7000",
          rationale: "Because"
        })
      }))
    );

    await expect(
      createForecast({
        eventId: "11111111-1111-4111-8111-111111111111",
        probability: 0.7,
        rationale: "Because"
      })
    ).resolves.toMatchObject({
      id: "22222222-2222-4222-8222-222222222222"
    });
  });
});

describe("ingestFromFdaBriefing", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.unstubAllGlobals();
  });

  it("returns parsed event on 201", async () => {
    vi.stubEnv("NEXT_PUBLIC_API_BASE_URL", "http://example.test");
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: true,
        json: async () => ({
          id: "33333333-3333-4333-8333-333333333333",
          title: "Drug Z PDUFA",
          kind: "fda_pdufa",
          drug_name: "Drug Z",
          sponsor: "Sponsor Z",
          indication: "Condition Z",
          decision_date: "2026-06-15",
          status: "upcoming",
          advisory_committee_date: null,
          primary_endpoint: null,
          advisory_committee_vote: null,
          source_url: "https://www.fda.gov/x.pdf"
        })
      }))
    );

    await expect(
      ingestFromFdaBriefing("https://www.fda.gov/drugs/doc.pdf")
    ).resolves.toMatchObject({
      id: "33333333-3333-4333-8333-333333333333",
      title: "Drug Z PDUFA",
      source_url: "https://www.fda.gov/x.pdf"
    });
  });

  it("throws ApiError with server message on failure", async () => {
    vi.stubEnv("NEXT_PUBLIC_API_BASE_URL", "http://example.test");
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: false,
        status: 400,
        json: async () => ({ error: "bad request: invalid pdf url" })
      }))
    );

    await expect(ingestFromFdaBriefing("https://www.fda.gov/x.pdf")).rejects.toMatchObject({
      name: "ApiError",
      status: 400,
      message: "bad request: invalid pdf url"
    });
  });
});

describe("fetchScoreSummary", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.unstubAllGlobals();
  });

  it("returns parsed score summary payload", async () => {
    vi.stubEnv("NEXT_PUBLIC_API_BASE_URL", "http://example.test");
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: true,
        json: async () => ({
          resolved_forecast_count: 2,
          total_brier: "0.13000000",
          mean_brier: "0.06500000",
          contributions: [
            {
              forecast_id: "22222222-2222-4222-8222-222222222222",
              event_id: "11111111-1111-4111-8111-111111111111",
              probability: "0.7000",
              occurred: true,
              brier_contribution: "0.09000000"
            }
          ]
        })
      }))
    );

    await expect(fetchScoreSummary()).resolves.toMatchObject({
      resolved_forecast_count: 2
    });
  });
});
