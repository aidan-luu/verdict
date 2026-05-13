import { describe, expect, it } from "vitest";

import {
  ingestFdaBriefingInputSchema,
  referenceClassResponseSchema
} from "./validators";

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

describe("referenceClassResponseSchema", () => {
  const baseResponse = {
    event_id: "11111111-1111-4111-8111-111111111111",
    current_features: {
      indication_area: "oncology",
      application_type: "NDA",
      primary_endpoint_type: "overall_survival",
      advisory_committee_held: true,
      has_any_feature: true
    },
    matches: [],
    aggregate: {
      sample_size: 0,
      approval_count: 0,
      crl_count: 0,
      base_rate: null,
      base_rate_absence_reason: "insufficient_sample" as const,
      enrichment_coverage_pct: 0
    }
  };

  it("accepts a complete response with base_rate populated", () => {
    const result = referenceClassResponseSchema.safeParse({
      ...baseResponse,
      matches: [
        {
          historical_event_id: "22222222-2222-4222-8222-222222222222",
          application_number: "NDA000001",
          drug_name: "Drug",
          sponsor_name: "Sponsor",
          application_type: "NDA",
          approval_date: "2024-01-01",
          indication_area: "oncology",
          primary_endpoint_type: "overall_survival",
          advisory_committee_held: true,
          advisory_committee_vote: "favorable",
          decision_outcome: "approved",
          enrichment_status: "llm_enriched",
          similarity_score: 0.7,
          match_reasons: ["indication_area", "primary_endpoint_type"]
        }
      ],
      aggregate: {
        sample_size: 11,
        approval_count: 6,
        crl_count: 5,
        base_rate: 6 / 11,
        base_rate_absence_reason: null,
        enrichment_coverage_pct: 100
      }
    });
    expect(result.success).toBe(true);
  });

  it("accepts a response with approval_only_bias absence reason and null base_rate", () => {
    const result = referenceClassResponseSchema.safeParse({
      ...baseResponse,
      aggregate: {
        sample_size: 10,
        approval_count: 10,
        crl_count: 0,
        base_rate: null,
        base_rate_absence_reason: "approval_only_bias" as const,
        enrichment_coverage_pct: 100
      }
    });
    expect(result.success).toBe(true);
  });

  it("rejects an unknown absence reason", () => {
    const result = referenceClassResponseSchema.safeParse({
      ...baseResponse,
      aggregate: {
        ...baseResponse.aggregate,
        base_rate_absence_reason: "made_up_reason"
      }
    });
    expect(result.success).toBe(false);
  });

  it("rejects an unknown match reason", () => {
    const result = referenceClassResponseSchema.safeParse({
      ...baseResponse,
      matches: [
        {
          historical_event_id: "22222222-2222-4222-8222-222222222222",
          application_number: "NDA000001",
          drug_name: "Drug",
          sponsor_name: "Sponsor",
          application_type: "NDA",
          approval_date: "2024-01-01",
          decision_outcome: "approved",
          enrichment_status: "llm_enriched",
          similarity_score: 0.5,
          match_reasons: ["mechanism_of_action"]
        }
      ]
    });
    expect(result.success).toBe(false);
  });
});
