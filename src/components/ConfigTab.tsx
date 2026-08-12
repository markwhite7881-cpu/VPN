import { FileJson, FolderOpen, Server } from "lucide-react";
import { Button } from "@/components/Button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/Card";
import { ConfigBuilder } from "@/components/ConfigBuilder";
import { ProxiesCard } from "@/components/ProxiesCard";
import { basename } from "@/lib/utils";
import { cn } from "@/lib/utils";
import type {
  BinaryInfo,
  GeneratorSettings,
  Outbound,
  SingboxVersion,
  Status,
} from "@/lib/types";

const inTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export interface ConfigTabProps {
  configPath: string;
  binary: BinaryInfo | null;
  version: SingboxVersion | null;
  statusLabel: Status;
  profiles: Outbound[];
  settings: GeneratorSettings;
  onSettingsChange: (next: GeneratorSettings) => void;
  onResetSettings: () => void;
  onPickConfig: () => void;
  onUseDefault: () => void;
  onStart: (configPath: string) => void;
  onConfigPath: (path: string | null) => void;
}

export function ConfigTab({
  configPath,
  binary,
  version,
  statusLabel,
  profiles,
  settings,
  onSettingsChange,
  onResetSettings,
  onPickConfig,
  onUseDefault,
  onStart,
  onConfigPath,
}: ConfigTabProps) {
  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-4 p-6">
      {/* Config picker */}
      <Card>
        <CardHeader>
          <div className="space-y-1">
            <CardTitle className="flex items-center gap-2">
              <FileJson className="h-4 w-4 text-muted-foreground" />
              Active config
            </CardTitle>
            <CardDescription>
              sing-box will be started with this file via{" "}
              <code className="rounded bg-muted px-1 py-0.5 text-[10px]">
                sing-box run -c &lt;path&gt;
              </code>
            </CardDescription>
          </div>
        </CardHeader>
        <CardContent className="space-y-2">
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              className="flex-1 justify-start truncate"
              onClick={onPickConfig}
            >
              <FolderOpen className="h-3.5 w-3.5" />
              <span className="truncate">
                {configPath ? basename(configPath) : "Pick config…"}
              </span>
            </Button>
            <Button variant="ghost" size="sm" onClick={onUseDefault}>
              Use default
            </Button>
          </div>
          {configPath && (
            <p className="break-all font-mono text-[10px] text-muted-foreground">
              {configPath}
            </p>
          )}
        </CardContent>
      </Card>

      {/* Config builder — composes the full sing-box config from the
          imported profiles and the routing toggles below it.
          Settings are lifted to App.tsx so they survive tab switches
          and a process restart (localStorage-backed). */}
      <ConfigBuilder
        profiles={profiles}
        settings={settings}
        onSettingsChange={onSettingsChange}
        onResetSettings={onResetSettings}
        onStart={onStart}
        onConfigPath={onConfigPath}
      />

      {/* Live: proxies (only useful when running; the home tab
          already shows live traffic, so we don't repeat it here). */}
      <ProxiesCard status={statusLabel} />

      {/* Binary info */}
      <Card>
        <CardHeader>
          <div className="space-y-1">
            <CardTitle className="flex items-center gap-2">
              <Server className="h-4 w-4 text-muted-foreground" />
              Binary
            </CardTitle>
            <CardDescription>Sidecar discovered by the backend</CardDescription>
          </div>
        </CardHeader>
        <CardContent className="space-y-2 text-xs">
          <Row label="Path" value={binary?.path || "—"} mono />
          <div className="grid grid-cols-3 gap-2">
            <Row
              label="Size"
              value={binary ? formatBytes(binary.size_bytes) : "—"}
              mono
            />
            <Row label="Exists" value={binary?.exists ? "yes" : "no"} mono />
            <Row
              label="Version"
              value={version?.version || "—"}
              mono
            />
          </div>
          {version && (
            <div className="mt-1 space-y-1 border-t border-border pt-2">
              <Row label="Revision" value={version.revision.slice(0, 16)} mono />
              <Row label="Env" value={version.environment} mono />
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function Row({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="space-y-0.5">
      <p className="text-[10px] uppercase tracking-wider text-muted-foreground">
        {label}
      </p>
      <p
        className={cn(
          "truncate",
          mono && "font-mono text-[11px]",
          !value || value === "—" ? "text-muted-foreground" : "text-foreground",
        )}
        title={value}
      >
        {value}
      </p>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (!bytes) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 100 || i === 0 ? 0 : 1)} ${units[i]}`;
}
