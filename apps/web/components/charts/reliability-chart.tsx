"use client";

import { Bar, BarChart, CartesianGrid, Legend, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";

type ReliabilityBucket = {
  bucket: string;
  predicted: number;
  observed: number;
};

type ReliabilityChartProps = {
  data: ReliabilityBucket[];
};

export function ReliabilityChart({ data }: ReliabilityChartProps) {
  return (
    <div className="h-80 w-full">
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={data}>
          <CartesianGrid strokeDasharray="3 3" />
          <XAxis dataKey="bucket" />
          <YAxis domain={[0, 1]} />
          <Tooltip />
          <Legend />
          <Bar dataKey="predicted" name="Avg forecast" fill="#2563eb" />
          <Bar dataKey="observed" name="Observed rate" fill="#16a34a" />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}
