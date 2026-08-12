import { useState } from "react";
import { Loader2, Plus, RefreshCw, Rss, Trash2 } from "lucide-react";
import { Button } from "./Button";
import { Badge } from "./Badge";
import { cn } from "@/lib/utils";
import type { Outbound, Subscription } from "@/lib/types";

export interface SubscriptionsCardProps {
  subs: Subscription[];
  fetching: Record<string, boolean>;
  onAdd: (input: { name?: string; url: string; intervalMinutes?: number }) => void;
  onRemove: (id: string) => void;
  onRefresh: (id: string) => void;
  onRefreshAll: () => void;
  onIntervalChange: (id: string, minutes: number) => void;
  onApply: (outbounds: Outbound[]) => void;
  /** Pre-aggregated outbounds from `lastResult` — passed in so the
   * parent can splice them into the main profiles list. */
  available: Outbound[];
}

export function SubscriptionsCard({
  subs,
  fetching,
  onAdd,
  onRemove,
  onRefresh,
  onRefreshAll,
  onIntervalChange,
  available,
}: SubscriptionsCardProps) {
  const [adding, setAdding] = useState(false);
  const [draftName, setDraftName] = useState("");
  const [draftUrl, setDraftUrl] = useState("");
  const [draftInterval, setDraftInterval] = useState(60);

  const onSubmit = () => {
    if (!draftUrl.trim()) return;
    onAdd({
      name: draftName.trim() || undefined,
      url: draftUrl.trim(),
      intervalMinutes: draftInterval,
    });
    setDraftName("");
    setDraftUrl("");
    setAdding(false);
  };

  return (
    <div className="rounded-lg border border-border bg-card text-card-foreground shadow-sm">
      <div className="flex flex-col space-y-1 p-5 pb-3">
        <div className="flex items-center justify-between">
          <h3 className="flex items-center gap-2 text-sm font-semibold tracking-tight text-foreground">
            <Rss className="h-4 w-4 text-muted-foreground" />
            Subscriptions
            <Badge variant="secondary" className="ml-1 px-1.5 py-0 text-[10px]">
                          </Badge>
            {subs.length > 0 && (
              <Badge variant="outline" className="px-1.5 py-0 text-[10px]">
                {subs.length}
              </Badge>
            )}
          </h3>
          <div className="flex gap-1">
            {subs.length > 0 && (
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                onClick={onRefreshAll}
                title="Refresh all"
              >
                <RefreshCw className="h-3.5 w-3.5" />
              </Button>
            )}
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={() => setAdding((v) => !v)}
              title="Add subscription"
            >
              <Plus className="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>
        <p className="text-xs text-muted-foreground">
          Pull vless/vmess/ss/... links from one or more URLs. Auto-refresh
          is on by default; profiles appear in the list above when fetched.
        </p>
      </div>
      <div className="space-y-2 p-5 pt-0">
        {adding && (
          <div className="space-y-2 rounded-md border border-border bg-card/40 p-3">
            <input
              type="text"
              placeholder="Name (optional)"
              value={draftName}
              onChange={(e) => setDraftName(e.target.value)}
              className="w-full rounded border border-input bg-background px-2 py-1.5 text-xs"
            />
            <input
              type="url"
              placeholder="https://provider.example.com/sub?token=…"
              value={draftUrl}
              onChange={(e) => setDraftUrl(e.target.value)}
              className="w-full rounded border border-input bg-background px-2 py-1.5 font-mono text-[11px]"
            />
            <div className="flex items-center gap-2">
              <label className="flex items-center gap-1.5 text-[10px] text-muted-foreground">
                refresh every
                <select
                  value={draftInterval}
                  onChange={(e) => setDraftInterval(parseInt(e.target.value, 10))}
                  className="rounded border border-input bg-background px-1 py-0.5 font-mono text-[11px]"
                >
                  <option value={0}>never</option>
                  <option value={15}>15 min</option>
                  <option value={30}>30 min</option>
                  <option value={60}>1 h</option>
                  <option value={360}>6 h</option>
                  <option value={1440}>daily</option>
                </select>
              </label>
              <Button
                size="sm"
                onClick={onSubmit}
                disabled={!draftUrl.trim()}
                className="ml-auto"
              >
                Add
              </Button>
            </div>
          </div>
        )}

        {subs.length === 0 && !adding && (
          <p className="rounded border border-border bg-card/40 p-3 text-[11px] text-muted-foreground">
            No subscriptions yet. Click <strong>+</strong> to add a URL.
            Stored locally — never uploaded.
          </p>
        )}

        <div className="space-y-1.5">
          {subs.map((s) => (
            <SubscriptionRow
              key={s.id}
              sub={s}
              loading={!!fetching[s.id]}
              onRefresh={() => onRefresh(s.id)}
              onRemove={() => onRemove(s.id)}
              onIntervalChange={(m) => onIntervalChange(s.id, m)}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

function SubscriptionRow({
  sub,
  loading,
  onRefresh,
  onRemove,
  onIntervalChange,
}: {
  sub: Subscription;
  loading: boolean;
  onRefresh: () => void;
  onRemove: () => void;
  onIntervalChange: (m: number) => void;
}) {
  const fetched = sub.lastFetchedAt
    ? new Date(sub.lastFetchedAt).toLocaleTimeString()
    : "—";
  return (
    <div className="rounded-md border border-border bg-card/40 p-2">
      <div className="flex items-center gap-2">
        <Rss className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        <span
          className="truncate text-sm font-medium"
          title={sub.name}
        >
          {sub.name}
        </span>
        {sub.lastCount > 0 && (
          <span className="rounded bg-muted px-1.5 py-0 font-mono text-[10px] text-muted-foreground">
            {sub.lastCount}
          </span>
        )}
        {sub.lastError && (
          <span
            className="rounded bg-destructive/10 px-1.5 py-0 text-[10px] text-destructive"
            title={sub.lastError}
          >
            {sub.lastErrorKind || "error"}
          </span>
        )}
        <div className="ml-auto flex items-center gap-1">
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={onRefresh}
            disabled={loading}
            title="Refresh now"
          >
            {loading ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <RefreshCw className="h-3.5 w-3.5" />
            )}
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={onRemove}
            title="Remove subscription"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>
      <div className="mt-1 flex items-center gap-2 text-[10px] text-muted-foreground">
        <span className="truncate font-mono" title={sub.url}>
          {sub.url}
        </span>
      </div>
      <div className="mt-1 flex items-center gap-2 text-[10px] text-muted-foreground">
        <span>last: {fetched}</span>
        <span>·</span>
        <span className={cn(sub.lastError ? "text-destructive" : "text-foreground/70")}>
          {sub.lastError ?? `${sub.lastCount} profile${sub.lastCount === 1 ? "" : "s"}`}
        </span>
        <span>·</span>
        <select
          value={sub.intervalMinutes}
          onChange={(e) => onIntervalChange(parseInt(e.target.value, 10))}
          className="rounded border border-input bg-background px-1 py-0 font-mono text-[10px]"
        >
          <option value={0}>never</option>
          <option value={15}>15m</option>
          <option value={30}>30m</option>
          <option value={60}>1h</option>
          <option value={360}>6h</option>
          <option value={1440}>24h</option>
        </select>
      </div>
    </div>
  );
}
