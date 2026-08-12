import { useEffect, useRef, useState } from "react";
import { Pause, Play, Trash2 } from "lucide-react";
import { Button } from "./Button";
import { cn } from "@/lib/utils";
import type { LogLine } from "@/lib/types";

interface Props {
  logs: LogLine[];
  onClear?: () => void;
  className?: string;
}

function formatTs(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleTimeString("en-GB", { hour12: false });
  } catch {
    return "--:--:--";
  }
}

const STREAM_COLOR: Record<LogLine["stream"], string> = {
  stdout: "text-foreground/80",
  stderr: "text-yellow-400",
  system: "text-primary",
};

export function LogView({ logs, onClear, className }: Props) {
  const [paused, setPaused] = useState(false);
  const [filter, setFilter] = useState<"all" | "stderr" | "system">("all");
  const scrollRef = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);

  useEffect(() => {
    if (paused) return;
    if (!stickToBottom.current) return;
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [logs, paused]);

  const onScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    const distance = el.scrollHeight - (el.scrollTop + el.clientHeight);
    stickToBottom.current = distance < 40;
  };

  const visible = filter === "all" ? logs : logs.filter((l) => l.stream === filter);

  return (
    <div className={cn("flex flex-col overflow-hidden rounded-md border border-border bg-card/40", className)}>
      <div className="flex items-center justify-between gap-2 border-b border-border bg-card/60 px-3 py-2">
        <div className="flex items-center gap-1.5">
          <button
            onClick={() => setFilter("all")}
            className={cn(
              "rounded px-2 py-0.5 text-[11px] font-medium",
              filter === "all"
                ? "bg-primary/20 text-primary"
                : "text-muted-foreground hover:text-foreground"
            )}
          >
            All
          </button>
          <button
            onClick={() => setFilter("stderr")}
            className={cn(
              "rounded px-2 py-0.5 text-[11px] font-medium",
              filter === "stderr"
                ? "bg-primary/20 text-primary"
                : "text-muted-foreground hover:text-foreground"
            )}
          >
            Errors
          </button>
          <button
            onClick={() => setFilter("system")}
            className={cn(
              "rounded px-2 py-0.5 text-[11px] font-medium",
              filter === "system"
                ? "bg-primary/20 text-primary"
                : "text-muted-foreground hover:text-foreground"
            )}
          >
            System
          </button>
        </div>
        <div className="flex items-center gap-1">
          <span className="text-[10px] text-muted-foreground">
            {visible.length} lines
          </span>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={() => setPaused((p) => !p)}
            title={paused ? "Resume autoscroll" : "Pause autoscroll"}
          >
            {paused ? <Play className="h-3.5 w-3.5" /> : <Pause className="h-3.5 w-3.5" />}
          </Button>
          {onClear && (
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={onClear}
              title="Clear (frontend only)"
            >
              <Trash2 className="h-3.5 w-3.5" />
            </Button>
          )}
        </div>
      </div>
      <div
        ref={scrollRef}
        onScroll={onScroll}
        className="flex-1 overflow-y-auto px-3 py-2 font-mono text-[11px] leading-relaxed"
        style={{ minHeight: 0 }}
      >
        {visible.length === 0 ? (
          <div className="flex h-full items-center justify-center text-muted-foreground">
            No log output yet.
          </div>
        ) : (
          visible.map((line, i) => (
            <div key={`${line.ts}-${i}`} className="flex gap-2">
              <span className="shrink-0 text-muted-foreground/60">
                {formatTs(line.ts)}
              </span>
              <span className={cn("shrink-0 uppercase opacity-60", STREAM_COLOR[line.stream])}>
                {line.stream[0]}
              </span>
              <span className={cn("break-all whitespace-pre-wrap", STREAM_COLOR[line.stream])}>
                {line.line}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
