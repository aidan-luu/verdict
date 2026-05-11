import { describe, expect, it } from "vitest";

import { ingestFdaBriefingInputSchema } from "./validators";

describe("ingestFdaBriefingInputSchema", () => {
  it("accepts https FDA-style URL", () => {
    const result = ingestFdaBriefingInputSchema.safeParse({
      pdfUrl: "https://www.fda.gov/drugs/foo.pdf"
    });
    expect(result.success).toBe(true);
  });

  it("accepts http localhost for dev stubs", () => {
    const result = ingestFdaBriefingInputSchema.safeParse({
      pdfUrl: "http://127.0.0.1:9999/test.pdf"
    });
    expect(result.success).toBe(true);
  });

  it("rejects plain http to non-loopback", () => {
    const result = ingestFdaBriefingInputSchema.safeParse({
      pdfUrl: "http://example.com/a.pdf"
    });
    expect(result.success).toBe(false);
  });

  it("rejects empty string", () => {
    const result = ingestFdaBriefingInputSchema.safeParse({ pdfUrl: "   " });
    expect(result.success).toBe(false);
  });
});
