import { afterEach, describe, expect, it, vi } from "vitest";

import { createForecast, fetchEvents, fetchHealth } from "./client";

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
