import Link from "next/link";

import { fetchScoreSummary } from "../../lib/api/client";
import type { ScoreContribution } from "../../lib/validators";
import { ReliabilityChart } from "../../components/charts/reliability-chart";

type ReliabilityBucket = {
  bucket: string;
  predictedSum: number;
  observedSum: number;
  count: number;
};

function buildReliabilityBuckets(contributions: ScoreContribution[]) {
  const buckets = new Map<number, ReliabilityBucket>();

  for (const contribution of contributions) {
    const probability = Number(contribution.probability);
    const bucketIndex = Math.min(9, Math.floor(probability * 10));
    const existing = buckets.get(bucketIndex) ?? {
      bucket: `${bucketIndex * 10}-${bucketIndex * 10 + 9}%`,
      predictedSum: 0,
      observedSum: 0,
      count: 0
    };

    existing.predictedSum += probability;
    existing.observedSum += contribution.occurred ? 1 : 0;
    existing.count += 1;
    buckets.set(bucketIndex, existing);
  }

  return Array.from(buckets.entries())
    .sort((left, right) => left[0] - right[0])
    .map(([, bucket]) => ({
      bucket: bucket.bucket,
      predicted: bucket.predictedSum / bucket.count,
      observed: bucket.observedSum / bucket.count
    }));
}

export default async function CalibrationPage() {
  try {
    const scoreSummary = await fetchScoreSummary();
    const chartData = buildReliabilityBuckets(scoreSummary.contributions);

    return (
      <main className="mx-auto max-w-5xl p-6">
        <h1 className="text-2xl font-semibold">Calibration dashboard</h1>
        <p className="mt-2 text-sm text-gray-700">
          Resolved forecasts: {scoreSummary.resolved_forecast_count}
        </p>
        <div className="mt-4 grid grid-cols-1 gap-3 md:grid-cols-2">
          <div className="rounded border p-4">
            <p className="text-sm text-gray-600">Total Brier</p>
            <p className="text-xl font-semibold">{scoreSummary.total_brier}</p>
          </div>
          <div className="rounded border p-4">
            <p className="text-sm text-gray-600">Mean Brier</p>
            <p className="text-xl font-semibold">{scoreSummary.mean_brier}</p>
          </div>
        </div>

        {chartData.length > 0 ? (
          <section className="mt-6 rounded border p-4">
            <h2 className="text-lg font-semibold">Reliability buckets</h2>
            <ReliabilityChart data={chartData} />
          </section>
        ) : (
          <p className="mt-6 rounded border p-4 text-sm">
            No resolved forecasts yet. Resolve events and add forecasts to populate calibration.
          </p>
        )}

        <p className="mt-6">
          <Link className="text-blue-700 underline" href="/">
            Back to events
          </Link>
        </p>
      </main>
    );
  } catch {
    return (
      <main className="mx-auto max-w-3xl p-6">
        <h1 className="text-2xl font-semibold">Calibration dashboard</h1>
        <p className="mt-2 text-sm">Could not load scoring data from the backend.</p>
      </main>
    );
  }
}
