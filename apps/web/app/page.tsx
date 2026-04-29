import { fetchHealth } from "../lib/api/client";

export default async function HomePage() {
  try {
    const health = await fetchHealth();

    return (
      <main className="max-w-3xl mx-auto p-6">
        <h1 className="text-2xl font-semibold">Verdict</h1>
        <p className="mt-2">Backend status: {health.status}</p>
      </main>
    );
  } catch (_error) {
    return (
      <main className="max-w-3xl mx-auto p-6">
        <h1 className="text-2xl font-semibold">Verdict</h1>
        <p className="mt-2">Backend status: unavailable</p>
      </main>
    );
  }
}
