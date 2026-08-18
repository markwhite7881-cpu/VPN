import { useState } from "react";
import { Link2, Loader2, Plus, Rss, Trash2 } from "lucide-react";
import { Button } from "@/components/Button";
import { Badge } from "@/components/Badge";
import { FlagIcon } from "@/components/FlagIcon";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/Card";
import { ProfileCard } from "@/components/ProfileCard";
import { SubscriptionsCard } from "@/components/SubscriptionsCard";
import { cn } from "@/lib/utils";
import type { HomeProfileMetadata, ParseFailure, SubscriptionSummary } from "@/lib/types";
import type { ConnectionProfile } from "@/lib/connectionProfiles";

export interface ServersTabProps {
  profiles: ConnectionProfile[];
  parseErrors: ParseFailure[];
  /** Top-level error from the last Parse attempt (e.g. command-not-found). */
  parseError: string | null;
  pendingLinks: string;
  onPendingLinksChange: (v: string) => void;
  onParse: () => void;
  onRemove: (index: number) => void;
  onClearAll: () => void;
  parsing: boolean;
  // Subscriptions
  subs: SubscriptionSummary[];
  subFetching: Record<string, boolean>;
  onAddSub: (sub: { url: string; name?: string; intervalMinutes?: number }) => void;
  onRemoveSub: (id: string) => void;
  onRefreshSub: (id: string) => void;
  onRefreshAllSubs: () => void;
  onSetSubInterval: (id: string, minutes: number) => void;
  onSelectSubChild: (subscriptionId: string, childKey: string) => void;
  /** ip → country code from the GeoIP cache (useGeoIp). */
  geoipByIp: Record<string, string>;
  /** Safe country metadata for ready Xray profiles, keyed by `${subscriptionId}:${key}`. */
  readyProfileMetadata: ReadonlyMap<string, HomeProfileMetadata>;
}

export function ServersTab({
  profiles,
  parseErrors,
  parseError,
  pendingLinks,
  onPendingLinksChange,
  onParse,
  onRemove,
  onClearAll,
  parsing,
  subs,
  subFetching,
  onAddSub,
  onRemoveSub,
  onRefreshSub,
  onRefreshAllSubs,
  onSetSubInterval,
  onSelectSubChild,
  geoipByIp,
  readyProfileMetadata,
}: ServersTabProps) {
  const [addOpen, setAddOpen] = useState(true);
  const [subOpen, setSubOpen] = useState(false);

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-4 p-6">
      {/* ─── Add: one input, two kinds of payload ─────────────────────
          Lines starting with `vless://`, `vmess://`, `trojan://`,
          `ss://`, `hy2://`, `tuic://` are parsed as share-links. Lines
          starting with `http://` or `https://` are added as
          subscription URLs. The Rust side auto-detects on parse. */}
      <Card>
        <CardHeader>
          <div className="flex items-start justify-between gap-2">
            <div className="space-y-1">
              <CardTitle className="flex items-center gap-2">
                <Link2 className="h-4 w-4 text-muted-foreground" />
                Add
              </CardTitle>
              <CardDescription>
                One entry per line — accepts share-links (vless://, vmess://,
                ss://, hy2://, trojan://, tuic://) and subscription URLs
                (https://…). Auto-detected.
              </CardDescription>
            </div>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setAddOpen((v) => !v)}
            >
              {addOpen ? "Hide" : "Add"}
            </Button>
          </div>
        </CardHeader>
        {addOpen && (
          <CardContent className="space-y-2">
            <textarea
              value={pendingLinks}
              onChange={(e) => onPendingLinksChange(e.target.value)}
              placeholder={
                "vless://uuid@host:port?type=tcp&security=reality&pbk=...\n" +
                "https://provider.example.com/sub?token=ABCD-1234"
              }
              className={cn(
                "min-h-[110px] w-full resize-y rounded-md border border-input bg-background/60 px-3 py-2",
                "font-mono text-[11px] leading-relaxed text-foreground placeholder:text-muted-foreground/50",
                "focus:border-ring focus:outline-none focus:ring-1 focus:ring-ring",
              )}
              spellCheck={false}
            />
            <div className="flex items-center gap-2">
              <Button
                size="sm"
                onClick={onParse}
                disabled={parsing || !pendingLinks.trim()}
                className="flex-1"
              >
                {parsing ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Plus className="h-3.5 w-3.5" />
                )}
                Add
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => onPendingLinksChange("")}
                title="Clear input"
              >
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            </div>
            {parseError && (
              <div className="rounded border border-destructive/30 bg-destructive/10 p-2 text-[11px] text-destructive">
                {parseError}
              </div>
            )}
            {parseErrors.length > 0 && (
              <div className="space-y-1 rounded border border-destructive/30 bg-destructive/10 p-2 text-[11px]">
                <p className="font-semibold text-destructive">
                  {parseErrors.length} parse error
                  {parseErrors.length === 1 ? "" : "s"}:
                </p>
                <ul className="space-y-0.5 pl-1 font-mono text-destructive/90">
                  {parseErrors.slice(0, 3).map((f, i) => (
                    <li key={i} className="truncate">
                      <span className="text-muted-foreground">
                        [{f.error.kind}]
                      </span>{" "}
                      {f.error.message}
                    </li>
                  ))}
                  {parseErrors.length > 3 && (
                    <li className="text-muted-foreground">
                      …and {parseErrors.length - 3} more
                    </li>
                  )}
                </ul>
              </div>
            )}
          </CardContent>
        )}
      </Card>

      {/* ─── Subscriptions (collapsible sub-section) ───────────────── */}
      <Card>
        <CardHeader>
          <div className="flex items-start justify-between gap-2">
            <div className="space-y-1">
              <CardTitle className="flex items-center gap-2">
                <Rss className="h-4 w-4 text-muted-foreground" />
                Subscriptions
                <Badge
                  variant="secondary"
                  className="px-1.5 py-0 text-[10px]"
                >
                  {subs.length}
                </Badge>
              </CardTitle>
              <CardDescription>
                Fetched periodically; profiles from each URL are merged
                into the list above.
              </CardDescription>
            </div>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setSubOpen((v) => !v)}
            >
              {subOpen ? "Hide" : "Manage"}
            </Button>
          </div>
        </CardHeader>
        {subOpen && (
          <CardContent>
            <SubscriptionsCard
              subs={subs}
              fetching={subFetching}
              onAdd={onAddSub}
              onRemove={onRemoveSub}
              onRefresh={onRefreshSub}
              onRefreshAll={onRefreshAllSubs}
              onIntervalChange={onSetSubInterval}
              onSelectChild={onSelectSubChild}
            />
          </CardContent>
        )}
      </Card>

      {/* ─── Profile list ───────────────────────────────────────────── */}
      <Card>
        <CardHeader>
          <div className="flex items-start justify-between gap-2">
            <div className="space-y-1">
              <CardTitle className="flex items-center gap-2">
                Servers
                <Badge
                  variant="secondary"
                  className="px-1.5 py-0 text-[10px]"
                >
                  {profiles.length}
                </Badge>
              </CardTitle>
              <CardDescription>
                Imported in the order they were added. The sing-box config
                generator wraps them in a selector + URLTest group.
              </CardDescription>
            </div>
            {profiles.length > 0 && (
              <Button
                size="sm"
                variant="ghost"
                onClick={onClearAll}
                title="Remove all manual servers (subscriptions stay)"
              >
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            )}
          </div>
        </CardHeader>
        <CardContent>
          {profiles.length === 0 ? (
            <div className="rounded-md border border-dashed border-border bg-card/30 p-6 text-center text-sm text-muted-foreground">
              No servers yet. Add a link or a subscription above.
            </div>
          ) : (
            <div className="space-y-1.5">
              {profiles.map((profile, i) =>
                profile.kind === "manual" ? (
                  <ProfileCard
                    key={`manual-${i}`}
                    outbound={profile.outbound}
                    geoipByIp={geoipByIp}
                    onRemove={() => onRemove(i)}
                  />
                ) : profile.kind === "subscription" ? (
                  <div
                    key={`subscription-${profile.reference.subscription_id}-${profile.reference.link_key}`}
                    className="rounded-md border border-border bg-card/40 p-3"
                  >
                    <div className="flex items-center gap-2">
                      <Badge variant="secondary" className="px-1.5 py-0 text-[10px]">
                        {profile.protocol}
                      </Badge>
                      <span className="truncate text-sm font-medium">{profile.label}</span>
                    </div>
                    <p className="mt-1.5 text-[11px] text-muted-foreground">
                      Subscription link — resolved securely when connecting.
                    </p>
                  </div>
                ) : (
                  <div
                    key={`ready-config-${profile.subscriptionId}-${profile.key}`}
                    className="rounded-md border border-border bg-card/40 p-3"
                  >
                    <div className="flex items-center gap-2">
                      {profile.engine === "xray" && (
                        <FlagIcon
                          code={readyProfileMetadata.get(`${profile.subscriptionId}:${profile.key}`)?.country_code ?? "??"}
                          size={16}
                          className="shrink-0"
                        />
                      )}
                      <Badge variant="secondary" className="px-1.5 py-0 text-[10px]">
                        {profile.engine === "singbox" ? "sing-box" : "Xray"}
                      </Badge>
                      <span className="truncate text-sm font-medium">{profile.name}</span>
                    </div>
                    <p className="mt-1.5 text-[11px] text-muted-foreground">
                      Ready configuration — selected for this subscription, execution is not available yet.
                    </p>
                  </div>
                ),
              )}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
