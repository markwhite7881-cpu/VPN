import { useState } from "react";
import { Loader2, Plus } from "lucide-react";
import { Sheet } from "./Sheet";
import { api } from "@/lib/api";
import type { Outbound } from "@/lib/types";
import { cn } from "@/lib/utils";
import { classifySourceInput } from "../lib/mobileUi";

const inputCls =
  "w-full rounded-md border border-input bg-background px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground/60 focus:outline-none focus:ring-1 focus:ring-ring";

export function AddSubscriptionSheet({
  open,
  onClose,
  onAdd,
  onAddLinks,
}: {
  open: boolean;
  onClose: () => void;
  onAdd: (input: { name?: string; url: string }) => void;
  onAddLinks: (outbounds: Outbound[]) => void;
}) {
  const [source, setSource] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const sourceKind = classifySourceInput(source).kind;

  const reset = () => {
    setSource("");
    setError(null);
    setBusy(false);
  };

  const submit = async () => {
    const input = source.trim();
    if (!input) {
      setError("Paste a subscription URL or share link.");
      return;
    }

    setBusy(true);
    setError(null);
    try {
      const result = await api.parseInput(source);
      if (result.outbounds.length === 0 && result.subscriptions.length === 0) {
        setError(
          result.failures[0]
            ? String(result.failures[0].error)
            : "Unsupported link or subscription URL.",
        );
        setBusy(false);
        return;
      }

      if (result.outbounds.length > 0) onAddLinks(result.outbounds);
      for (const url of result.subscriptions) onAdd({ url });

      reset();
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  };

  const sourceHint =
    sourceKind === "share"
      ? "Share link detected. Additional lines are parsed too."
      : sourceKind === "subscription"
        ? "Subscription URL detected. Additional lines are parsed too."
        : "Paste one or more links. Each non-empty line will be parsed.";

  return (
    <Sheet open={open} onClose={onClose} title="Add servers">
      <div className="space-y-3">
        <div>
          <label className="mb-1 block text-xs text-muted-foreground" htmlFor="server-source">
            Source
          </label>
          <textarea
            id="server-source"
            value={source}
            onChange={(e) => {
              setSource(e.target.value);
              setError(null);
            }}
            placeholder={"Paste a subscription URL or share link\nhttps://provider.example/sub\nvless://…"}
            rows={5}
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            className={cn(inputCls, "resize-none font-mono text-xs")}
          />
          <p className="mt-1 text-[11px] text-muted-foreground/70">{sourceHint}</p>
        </div>

        {error && <p className="text-xs text-destructive">{error}</p>}
        <button
          type="button"
          onClick={() => void submit()}
          disabled={busy || source.trim().length === 0}
          className={cn(
            "flex h-10 w-full items-center justify-center gap-2 rounded-md text-sm font-medium",
            "bg-primary text-primary-foreground shadow-sm transition-colors hover:bg-primary/90",
            "disabled:pointer-events-none disabled:opacity-50",
          )}
        >
          {busy ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Plus className="h-4 w-4" />
          )}
          Add servers
        </button>
      </div>
    </Sheet>
  );
}
