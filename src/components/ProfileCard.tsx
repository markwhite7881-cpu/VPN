import { useState } from "react";
import { Check, Copy, Eye, EyeOff, Globe2, Server, X } from "lucide-react";
import { Badge } from "./Badge";
import { Button } from "./Button";
import { FlagIcon } from "./FlagIcon";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";
import { flagForProfile } from "@/lib/flags";
import { isSupported } from "@/lib/outbound";
import type { Outbound, TlsCfg, Transport } from "@/lib/types";

const PROTOCOL_VARIANT: Record<string, "default" | "secondary" | "destructive" | "outline"> = {
  vless: "default",
  vmess: "secondary",
  trojan: "secondary",
  shadowsocks: "secondary",
  hysteria2: "outline",
  tuic: "outline",
  unsupported: "destructive",
};

interface Props {
  outbound: Outbound;
  /** ip → country code from the GeoIP cache. */
  geoipByIp?: Record<string, string>;
  onRemove: () => void;
}

// Fallback sing-box JSON serialiser for browser preview (no Tauri).
// Best-effort — the Rust impl is the source of truth.
function previewToSingboxJson(o: Outbound): Record<string, unknown> {
  if (o.protocol === "unsupported") {
    return { type: "block", tag: "unsupported-placeholder" };
  }
  const out: Record<string, unknown> = {
    type: o.protocol,
    tag: o.tag,
    server: o.server,
    server_port: o.port,
  };
  if ("uuid" in o) out.uuid = o.uuid;
  if ("password" in o) out.password = o.password;
  if ("flow" in o && o.flow) out.flow = o.flow;
  if ("alter_id" in o) out.alter_id = o.alter_id;
  if ("method" in o) out.method = o.method;
  if ("congestion_control" in o) out.congestion_control = o.congestion_control;
  if ("udp_relay_mode" in o) out.udp_relay_mode = o.udp_relay_mode;
  if ("transport" in o) out.transport = o.transport;
  if ("obfs" in o && o.obfs) out.obfs = o.obfs;
  if ("tls" in o && o.tls?.enabled) {
    const tls: Record<string, unknown> = { enabled: true };
    if (o.tls.server_name) tls.server_name = o.tls.server_name;
    if (o.tls.alpn?.length) tls.alpn = o.tls.alpn;
    if (o.tls.fingerprint) tls.utls = { fingerprint: o.tls.fingerprint };
    if (o.tls.reality) {
      tls.reality = {
        enabled: true,
        public_key: o.tls.reality.public_key,
        short_id: o.tls.reality.short_id,
      };
    }
    if (o.tls.allow_insecure) tls.insecure = true;
    out.tls = tls;
  }
  return out;
}

function getDisplayName(o: Outbound): string {
  switch (o.protocol) {
    case "vless":
    case "vmess":
    case "trojan":
    case "shadowsocks":
    case "hysteria2":
    case "tuic":
      return o.tag || `${o.server}:${o.port}`;
    case "unsupported":
      return "Unsupported link";
  }
}

function getServerInfo(o: Outbound): string {
  if (o.protocol === "unsupported") return o.reason;
  return `${o.server}:${o.port}`;
}

function summarizeTransport(t: Transport): string | null {
  switch (t.kind) {
    case "tcp":
      return null;
    case "ws":
      return t.path ? `ws ${t.path}` : "ws";
    case "http":
      return t.path ? `http ${t.path}` : "http";
    case "xhttp":
      return t.path ? `xhttp ${t.path}` : "xhttp";
    case "grpc":
      return t.service_name ? `grpc ${t.service_name}` : "grpc";
    case "udp":
      return "udp";
  }
}

function summarizeTls(t: TlsCfg): string | null {
  if (!t.enabled && !t.reality) return null;
  const parts: string[] = [];
  if (t.reality) parts.push("Reality");
  else if (t.enabled) parts.push("TLS");
  if (t.server_name) parts.push(t.server_name);
  if (t.fingerprint) parts.push(`fp=${t.fingerprint}`);
  if (t.allow_insecure) parts.push("insecure");
  return parts.join(" · ");
}

function protocolTag(o: Outbound): string {
  switch (o.protocol) {
    case "shadowsocks":
      return "SS";
    default:
      return o.protocol;
  }
}

function protocolExtra(o: Outbound): string | null {
  switch (o.protocol) {
    case "vless":
      return o.flow === "xtls-rprx-vision" ? "vision" : null;
    case "hysteria2":
      return o.obfs ? `obfs=${o.obfs.type}` : null;
    case "shadowsocks":
      return o.method;
    case "tuic":
      return o.congestion_control;
    default:
      return null;
  }
}

export function ProfileCard({ outbound, geoipByIp, onRemove }: Props) {
  const [showJson, setShowJson] = useState(false);
  const [jsonText, setJsonText] = useState<string | null>(null);
  const [jsonError, setJsonError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const variant = PROTOCOL_VARIANT[outbound.protocol] ?? "secondary";
  const name = getDisplayName(outbound);
  const server = getServerInfo(outbound);
  const transport =
    "transport" in outbound
      ? summarizeTransport(outbound.transport as Transport)
      : null;
  const tls = "tls" in outbound ? summarizeTls(outbound.tls as TlsCfg) : null;
  const extra = protocolExtra(outbound);
  const flag = flagForProfile({
    tag: isSupported(outbound) ? outbound.tag : undefined,
    server: isSupported(outbound) ? outbound.server : undefined,
    geoipByIp,
  });

  const onToggleJson = async () => {
    if (showJson) {
      setShowJson(false);
      return;
    }
    setShowJson(true);
    if (jsonText) return;
    setJsonError(null);
    try {
      // Preview mode has no Tauri invoke; build a best-effort JSON locally
      // so the UI stays demoable. In the real shell the Rust side emits
      // the canonical sing-box schema.
      const inTauri =
        typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
      let v: unknown;
      if (inTauri) {
        v = await api.outboundToSingboxJson(outbound);
      } else {
        v = previewToSingboxJson(outbound);
      }
      setJsonText(JSON.stringify(v, null, 2));
    } catch (e) {
      setJsonError(String(e));
    }
  };

  const onCopyJson = async () => {
    if (!jsonText) return;
    try {
      await navigator.clipboard.writeText(jsonText);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // ignore
    }
  };

  return (
    <div
      className={cn(
        "group rounded-md border border-border bg-card/40 p-3 transition-colors",
        "hover:border-primary/30"
      )}
    >
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <Badge variant={variant} className="px-1.5 py-0 text-[10px]">
              {protocolTag(outbound)}
            </Badge>
            <FlagIcon
              code={flag.code}
              size={18}
              className="shrink-0 self-center"
              alt={flag.code}
            />
            <span className="truncate text-sm font-medium" title={name}>
              {name}
            </span>
            {extra && (
              <span className="rounded bg-muted px-1.5 py-0 font-mono text-[10px] text-muted-foreground">
                {extra}
              </span>
            )}
          </div>
          <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
            <span className="inline-flex items-center gap-1">
              <Server className="h-3 w-3" />
              <span className="font-mono">{server}</span>
            </span>
            {transport && (
              <span className="inline-flex items-center gap-1">
                <Globe2 className="h-3 w-3" />
                {transport}
              </span>
            )}
            {tls && (
              <span className="inline-flex items-center gap-1 font-mono">
                {tls}
              </span>
            )}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-0.5 opacity-60 transition-opacity group-hover:opacity-100">
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={onToggleJson}
            title={showJson ? "Hide JSON" : "Show sing-box JSON"}
          >
            {showJson ? (
              <EyeOff className="h-3.5 w-3.5" />
            ) : (
              <Eye className="h-3.5 w-3.5" />
            )}
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={onRemove}
            title="Remove"
          >
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>

      {showJson && (
        <div className="mt-2 overflow-hidden rounded border border-border bg-background/50">
          {jsonError ? (
            <div className="p-2 text-[11px] text-destructive">
              failed to render: {jsonError}
            </div>
          ) : (
            <>
              <pre className="max-h-64 overflow-auto px-3 py-2 font-mono text-[10.5px] leading-relaxed text-foreground/80">
                {jsonText ?? "rendering…"}
              </pre>
              <div className="flex items-center justify-end border-t border-border bg-card/40 px-2 py-1">
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={onCopyJson}
                  className="h-6 px-2 text-[10px]"
                >
                  {copied ? (
                    <>
                      <Check className="h-3 w-3" /> Copied
                    </>
                  ) : (
                    <>
                      <Copy className="h-3 w-3" /> Copy JSON
                    </>
                  )}
                </Button>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}
