// UpdateCard — combined UI for the two auto-update flows.
//
//   1. App shell (Tauri updater). Source: GitHub Releases, signed
//      with the project's `tauri-updater.key`. The frontend uses
//      `@tauri-apps/plugin-updater` directly — no Rust command in
//      between. The manifest lives at
//      `https://github.com/markwhite7881-cpu/VPN/releases/latest/
//      download/latest.json` (see tauri.conf.json).
//
//   2. sing-box core (custom Rust commands, see src-tauri/src/
//      updates.rs). The frontend calls `api.checkSingboxUpdate` /
//      `api.applySingboxUpdate` and renders the result.
//
// Why two separate cards in one component? Both update flows are
// conceptually similar (check → show diff → install) and a
// non-tech-savvy user benefits from seeing them together at the
// bottom of the Home tab — "what's the latest, what do I have".
//
// Behaviour:
//   - On mount, both checks fire automatically (silent in the
//     background). The "Check for updates" buttons let the user
//     force a refresh.
//   - The Tauri updater shows a one-click "Restart and update"
//     button when an update is available. `downloadAndInstall`
//     blocks until the download is finished; the manual restart
//     happens via `app.restart()`.
//   - The sing-box updater shows the current → latest version
//     and a "Download" button. The actual install runs in Rust
//     (stops the running process, downloads the .zip, extracts
//     the binary, places it at <app_data_dir>/singbox-runtime/).
//
// Both flows are no-ops when running outside the Tauri shell
// (browser preview), so the card stays quiet in dev.

import { useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
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
  const [appUpdate, setAppUpdate] = useState<Update | null>(null);
  const [appBusy, setAppBusy] = useState(false);
  const [appError, setAppError] = useState<string | null>(null);

  // sing-box (custom Rust) state.
  const [sbUpdate, setSbUpdate] = useState<{
    latest: string;
    downloadUrl: string | null;
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
      const u = await check();
      setAppUpdate(u ?? null);
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
      // `downloadAndInstall` is from the updater plugin; the
      // type isn't on `Update` itself but the runtime method
      // exists. Cast to keep the call site readable.
      await (appUpdate as unknown as { downloadAndInstall: () => Promise<void> })
        .downloadAndInstall();
      // The Tauri updater requires a manual relaunch — it
      // can't replace the running .exe while it's mapped in.
      // `relaunch()` (from plugin-process) re-execs the current
      // binary; on Windows that's typically a one-second flicker.
      await relaunch();
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
      if (info.available && info.download_url) {
        setSbUpdate({
          latest: info.latest_version,
          downloadUrl: info.download_url,
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
    if (!sbUpdate?.downloadUrl) return;
    setSbBusy(true);
    setSbError(null);
    try {
      await api.applySingboxUpdate(sbUpdate.downloadUrl);
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
  update: Update | null;
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
                {update!.body ? (
                  <span className="text-xs text-muted-foreground"> — restart to apply</span>
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
