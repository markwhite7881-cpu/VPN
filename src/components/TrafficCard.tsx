import { useEffect, useRef, useState } from "react";
import { Activity, ArrowDownToLine, ArrowUpToLine } from "lucide-react";
import { Badge } from "./Badge";
import { cn } from "@/lib/utils";
import { useTrafficStream } from "@/hooks/useTrafficStream";
import type { Status, TrafficSample } from "@/lib/types";

interface Props {
  status: Status;
  profileCount: number;
  /**
   * When true, show the demo chart even if `status` is not "running".
   * Used by the browser preview so the chart is always visible.
   */
  forceDemo?: boolean;
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B/s`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB/s`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(2)} MB/s`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB/s`;
}

function formatTotal(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(2)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

/**
 * Build an SVG `d` path from a list of values mapped to a width/height.
 * Falls back to a flat line at zero when there are fewer than two samples.
 */
function buildPath(
  values: number[],
  width: number,
  height: number,
  max: number,
): string {
  if (values.length < 2 || max <= 0) {
    return `M 0 ${height} L ${width} ${height}`;
  }
  const stepX = width / (values.length - 1);
  let d = "";
  for (let i = 0; i < values.length; i++) {
    const x = i * stepX;
    const y = height - (values[i] / max) * height;
    d += i === 0 ? `M ${x.toFixed(2)} ${y.toFixed(2)}` : ` L ${x.toFixed(2)} ${y.toFixed(2)}`;
  }
  return d;
}

function buildAreaPath(
  values: number[],
  width: number,
  height: number,
  max: number,
): string {
  const linePath = buildPath(values, width, height, max);
  if (!linePath) return "";
  return `${linePath} L ${width} ${height} L 0 ${height} Z`;
}

const WIDTH = 560;
const HEIGHT = 80;

export function TrafficCard({ status, profileCount, forceDemo }: Props) {
  const live = status === "running" || !!forceDemo;
  const { samples, current } = useTrafficStream(live, profileCount);
  const containerRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(WIDTH);

  useEffect(() => {
    if (!containerRef.current) return;
    const ro = new ResizeObserver((entries) => {
      for (const e of entries) {
        const w = Math.max(280, Math.floor(e.contentRect.width));
        setWidth(w);
      }
    });
    ro.observe(containerRef.current);
    return () => ro.disconnect();
  }, []);

  const downSeries = samples.map((s) => s.down_bps);
  const upSeries = samples.map((s) => s.up_bps);
  const maxDown = Math.max(1, ...downSeries);
  const maxUp = Math.max(1, ...upSeries);

  return (
    <div className="rounded-lg border border-border bg-card text-card-foreground shadow-sm">
      <div className="flex flex-col space-y-1 p-5 pb-3">
        <div className="flex items-center justify-between">
          <h3 className="flex items-center gap-2 text-sm font-semibold tracking-tight text-foreground">
            <Activity className="h-4 w-4 text-muted-foreground" />
            Traffic
            <Badge variant="secondary" className="ml-1 px-1.5 py-0 text-[10px]">
                          </Badge>
          </h3>
          {live ? (
            <Badge variant="default" className="px-1.5 py-0 text-[10px]">
              live
            </Badge>
          ) : (
            <Badge variant="outline" className="px-1.5 py-0 text-[10px]">
              not running
            </Badge>
          )}
        </div>
        <p className="text-xs text-muted-foreground">
          Live upload / download speed, fed by sing-box's{" "}
          <code className="font-mono">/traffic</code> WebSocket.
        </p>
      </div>
      <div className="space-y-3 p-5 pt-0">
        {!live ? (
          <p className="rounded border border-border bg-card/40 p-3 text-[11px] text-muted-foreground">
            Connect to start streaming. The chart fills as samples arrive
            from the WebSocket.
          </p>
        ) : (
          <>
            <div className="grid grid-cols-2 gap-3">
              <RateBox
                label="Download"
                value={current?.down_bps ?? 0}
                total={current?.down_total ?? 0}
                direction="down"
              />
              <RateBox
                label="Upload"
                value={current?.up_bps ?? 0}
                total={current?.up_total ?? 0}
                direction="up"
              />
            </div>
            <div
              ref={containerRef}
              className="rounded border border-border bg-card/40 p-3"
            >
              <Chart
                title="Download"
                values={downSeries}
                width={width}
                height={HEIGHT}
                max={maxDown}
                tone="foreground"
              />
              <div className="mt-3" />
              <Chart
                title="Upload"
                values={upSeries}
                width={width}
                height={HEIGHT}
                max={maxUp}
                tone="muted"
              />
              <p className="mt-2 text-[10px] text-muted-foreground">
                last {samples.length} samples · max down{" "}
                {formatBytes(maxDown)} · max up {formatBytes(maxUp)}
              </p>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function RateBox({
  label,
  value,
  total,
  direction,
}: {
  label: string;
  value: number;
  total: number;
  direction: "up" | "down";
}) {
  const Icon = direction === "down" ? ArrowDownToLine : ArrowUpToLine;
  return (
    <div className="rounded border border-border bg-card/40 p-3">
      <div className="flex items-center gap-1.5 text-[10px] uppercase tracking-wider text-muted-foreground">
        <Icon className="h-3 w-3" />
        {label}
      </div>
      <div className="mt-1 font-mono text-lg font-medium tabular-nums">
        {formatBytes(value)}
      </div>
      <div className="mt-0.5 text-[10px] text-muted-foreground">
        total: {formatTotal(total)}
      </div>
    </div>
  );
}

function Chart({
  title,
  values,
  width,
  height,
  max,
  tone,
}: {
  title: string;
  values: number[];
  width: number;
  height: number;
  max: number;
  tone: "foreground" | "muted";
}) {
  const line = buildPath(values, width, height, max);
  const area = buildAreaPath(values, width, height, max);
  const stroke = tone === "foreground" ? "hsl(var(--foreground))" : "hsl(var(--muted-foreground))";
  const fill = tone === "foreground" ? "hsl(var(--foreground) / 0.08)" : "hsl(var(--muted-foreground) / 0.08)";
  return (
    <div>
      <div className="mb-1 flex items-center justify-between text-[10px] uppercase tracking-wider text-muted-foreground">
        <span>{title}</span>
        <span className="font-mono normal-case tabular-nums">
          {values.length > 0 ? formatBytes(values[values.length - 1]) : "—"}
        </span>
      </div>
      <svg
        viewBox={`0 0 ${width} ${height}`}
        width="100%"
        height={height}
        preserveAspectRatio="none"
        className={cn("overflow-visible")}
      >
        {area && <path d={area} fill={fill} stroke="none" />}
        <path
          d={line}
          fill="none"
          stroke={stroke}
          strokeWidth={1.5}
          strokeLinecap="round"
          strokeLinejoin="round"
          vectorEffect="non-scaling-stroke"
        />
      </svg>
    </div>
  );
}
