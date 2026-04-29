import { afterEach, describe, expect, it, vi } from "vitest";

import { fetchHealth } from "./client";

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
