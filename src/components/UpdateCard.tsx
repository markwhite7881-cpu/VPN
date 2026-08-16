// UpdateCard — combined UI for backend-verified app-shell and sing-box updates.
//
// The app-shell manifest, artifact URL, and minisign signature remain in Rust.
// The WebView receives only version, current version, availability, and notes.
// The sing-box core remains a separate custom Rust update flow.

import { useEffect, useState } from "react";
import { Download, RefreshCw, ShieldCheck, Cpu } from "lucide-react";
import { Button } from "./Button";
import { api, TauriCommandError } from "@/lib/api";
import { cn } from "@/lib/utils";

interface Props {
  /** Currently-running sing-box version, fetched by App.tsx. */
  currentSingboxVersion: string | null;
  /**
   * Called after the sing-box has been auto-updated. The parent
   * refetches `get_singbox_version` so the new value is shown
   * everywhere (status pill, logs, etc.).
   */
  onSingboxUpdated: () => void;
}

export function UpdateCard({ currentSingboxVersion, onSingboxUpdated }: Props) {
  // App shell (Tauri updater) state.
  const [appUpdate, setAppUpdate] = useState<{
    version: string;
    current_version: string;
    available: boolean;
    notes: string;
  } | null>(null);
  const [appBusy, setAppBusy] = useState(false);
  const [appError, setAppError] = useState<string | null>(null);

  // sing-box (custom Rust) state.
  const [sbUpdate, setSbUpdate] = useState<{
    latest: string;
    sizeBytes: number;
  } | null>(null);
  const [sbBusy, setSbBusy] = useState(false);
  const [sbError, setSbError] = useState<string | null>(null);

  // Auto-check both on mount.
  useEffect(() => {
    void checkAppUpdate();
    void checkSbUpdate();
    // We intentionally don't re-check on prop change — the user
    // has the manual "Check" buttons for that.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const checkAppUpdate = async () => {
    setAppError(null);
    try {
      const update = await api.checkAppUpdate();
      setAppUpdate(update.available ? update : null);
    } catch (e) {
      // `check()` throws when no update is available OR on a
      // network error. Both look the same to the caller, so we
      // surface a generic "check failed" message and let the
      // user retry.
      const msg =
        e instanceof TauriCommandError
          ? `${e.kind}: ${e.message}`
          : e instanceof Error
            ? e.message
            : String(e);
      setAppError(msg);
    }
  };

  const installAppUpdate = async () => {
    if (!appUpdate) return;
    setAppBusy(true);
    setAppError(null);
    try {
      await api.installAppUpdate(appUpdate.version);
    } catch (e) {
      const msg =
        e instanceof TauriCommandError
          ? `${e.kind}: ${e.message}`
          : e instanceof Error
            ? e.message
            : String(e);
      setAppError(msg);
    } finally {
      setAppBusy(false);
    }
  };

  const checkSbUpdate = async () => {
    setSbError(null);
    try {
      const info = await api.checkSingboxUpdate();
      if (info.available) {
        setSbUpdate({
          latest: info.latest_version,
          sizeBytes: info.size_bytes,
        });
      } else {
        setSbUpdate(null);
      }
    } catch (e) {
      const msg =
        e instanceof TauriCommandError
          ? `${e.kind}: ${e.message}`
          : e instanceof Error
            ? e.message
            : String(e);
      setSbError(msg);
    }
  };

  const installSbUpdate = async () => {
    if (!sbUpdate) return;
    setSbBusy(true);
    setSbError(null);
    try {
      await api.applySingboxUpdate(sbUpdate.latest);
      // The Rust side placed a new binary at
      // <app_data_dir>/singbox-runtime/. ProcessManager now
      // prefers that path on next start. We re-fetch the
      // version so the UI reflects reality.
      onSingboxUpdated();
      setSbUpdate(null);
    } catch (e) {
      const msg =
        e instanceof TauriCommandError
          ? `${e.kind}: ${e.message}`
          : e instanceof Error
            ? e.message
            : String(e);
      setSbError(msg);
    } finally {
      setSbBusy(false);
    }
  };

  return (
    <div className="rounded-md border border-border bg-card/30 p-4 space-y-3">
      <div className="flex items-start gap-2.5">
        <ShieldCheck size={16} className="mt-0.5 text-muted-foreground" />
        <div className="min-w-0">
          <h3 className="text-sm font-medium text-foreground">Updates</h3>
          <p className="text-xs text-muted-foreground mt-0.5">
            App shell and VPN core. Auto-checked on launch.
          </p>
        </div>
      </div>

      <AppUpdateRow
        update={appUpdate}
        busy={appBusy}
        error={appError}
        onCheck={checkAppUpdate}
        onInstall={installAppUpdate}
      />

      <SingboxUpdateRow
        currentVersion={currentSingboxVersion}
        latestVersion={sbUpdate?.latest ?? null}
        sizeBytes={sbUpdate?.sizeBytes ?? 0}
        busy={sbBusy}
        error={sbError}
        onCheck={checkSbUpdate}
        onInstall={installSbUpdate}
      />
    </div>
  );
}

// ---- sub-rows -------------------------------------------------------

function AppUpdateRow({
  update,
  busy,
  error,
  onCheck,
  onInstall,
}: {
  update: {
    version: string;
    current_version: string;
    available: boolean;
    notes: string;
  } | null;
  busy: boolean;
  error: string | null;
  onCheck: () => void;
  onInstall: () => void;
}) {
  const available = !!update;
  return (
    <div className="rounded border border-border bg-background/40 p-3">
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <div className="min-w-0">
          <div className="text-xs text-muted-foreground">App</div>
          <div className="text-sm font-medium text-foreground">
            {available ? (
              <>
                New version{" "}
                <span className="font-mono text-primary">{update!.version}</span>{" "}
                available
                {update!.notes ? (
                  <span className="text-xs text-muted-foreground"> — {update!.notes}</span>
                ) : null}
              </>
            ) : error ? (
              <span className="text-muted-foreground">Check failed</span>
            ) : (
              <span className="text-muted-foreground">Up to date</span>
            )}
          </div>
        </div>
        <div className="flex items-center gap-1.5">
          <Button
            variant="ghost"
            size="sm"
            onClick={onCheck}
            disabled={busy}
            title="Check for app updates"
          >
            <RefreshCw size={12} className={busy ? "animate-spin" : ""} />
          </Button>
          {available && (
            <Button size="sm" onClick={onInstall} disabled={busy}>
              <Download size={12} className="mr-1" />
              {busy ? "Installing…" : "Update & restart"}
            </Button>
          )}
        </div>
      </div>
      {error && (
        <div className="mt-1.5 text-[11px] text-destructive-foreground/80 font-mono break-all">
          {error}
        </div>
      )}
    </div>
  );
}

function SingboxUpdateRow({
  currentVersion,
  latestVersion,
  sizeBytes,
  busy,
  error,
  onCheck,
  onInstall,
}: {
  currentVersion: string | null;
  latestVersion: string | null;
  sizeBytes: number;
  busy: boolean;
  error: string | null;
  onCheck: () => void;
  onInstall: () => void;
}) {
  const available = !!latestVersion && latestVersion !== currentVersion;
  return (
    <div className="rounded border border-border bg-background/40 p-3">
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <div className="min-w-0">
          <div className="text-xs text-muted-foreground flex items-center gap-1">
            <Cpu size={11} />
            VPN core
          </div>
          <div className="text-sm font-medium text-foreground">
            {currentVersion ? (
              <span className="font-mono">{currentVersion}</span>
            ) : (
              <span className="text-muted-foreground">not detected</span>
            )}
            {available && latestVersion && (
              <>
                <span className="text-muted-foreground mx-1">→</span>
                <span className="font-mono text-primary">{latestVersion}</span>
              </>
            )}
            {!available && !error && currentVersion && (
              <span className="text-muted-foreground ml-2">— up to date</span>
            )}
            {error && (
              <span className="text-muted-foreground ml-2">— check failed</span>
            )}
          </div>
        </div>
        <div className="flex items-center gap-1.5">
          <Button
            variant="ghost"
            size="sm"
            onClick={onCheck}
            disabled={busy}
            title="Check for sing-box updates"
          >
            <RefreshCw size={12} className={busy ? "animate-spin" : ""} />
          </Button>
          {available && (
            <Button size="sm" onClick={onInstall} disabled={busy}>
              <Download size={12} className="mr-1" />
              {busy
                ? "Installing…"
                : `Download${sizeBytes ? ` ${formatBytes(sizeBytes)}` : ""}`}
            </Button>
          )}
        </div>
      </div>
      {error && (
        <div className="mt-1.5 text-[11px] text-destructive-foreground/80 font-mono break-all">
          {error}
        </div>
      )}
    </div>
  );
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

// `cn` re-export is unused in this file but kept for future tweaks
// (e.g. conditional classes for the install-in-progress state).
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _cn = cn;
