# Xray geodata provisioning design

**Date:** 2026-08-18
**Status:** approved for implementation

## Goal

Allow Cloakwire's Xray fallback profiles that reference `geoip:*` or `geosite:*` rules to work on desktop without requiring a user to copy data files manually. `sing-box` remains the primary engine and its behavior is unchanged.

## Scope

- Desktop Xray fallback only.
- Persist `geoip.dat` and `geosite.dat` in the Cloakwire per-user application-data directory.
- Obtain data only from GitHub Release assets of `Loyalsoldier/v2ray-rules-dat`, the upstream selected by Xray's installation and build automation.
- Verify the hash published for each data file before it becomes usable.
- Reuse the last complete verified pair while a periodic update check cannot reach GitHub.

Out of scope: bundling geodata into application releases, accepting data URLs or checksums from the WebView, silently using unverified mirrors, changing subscription format, changing routing behavior, or Android support.

## Storage

The canonical directory is:

```text
<app_data_dir>/xray-geodata/
  geoip.dat
  geosite.dat
  state.json
```

`state.json` records only the successful check time and the verified release identity/checksums. It contains no subscription information, profile content, URLs, credentials, HWID, or server addresses.

Files are downloaded to unique temporary names in the same directory. They are verified in memory/on disk first, then atomically renamed into the canonical pair. A failed download never replaces a known-good file.

## Source and trust policy

The application accepts only plain HTTPS GitHub release URLs under:

```text
https://github.com/Loyalsoldier/v2ray-rules-dat/releases/download/<tag>/
```

The release API response supplies the release tag and asset metadata. The implementation binds file names exactly to `geoip.dat`, `geosite.dat`, `geoip.dat.sha256sum`, and `geosite.dat.sha256sum`; rejects credentials, query strings, fragments, unexpected hosts, unexpected asset names, unsafe redirects, oversized responses, and malformed checksums.

For each data file, Cloakwire downloads its matching `.sha256sum`, parses exactly one expected SHA-256 entry for that file, downloads the data file, and compares the computed SHA-256 in constant time before persistence. A release asset digest may be used as an additional consistency check when GitHub provides one, but the checked-in policy must not rely on an optional API field.

Redirects follow the existing release-download policy: GitHub release route to GitHub release-asset storage only, HTTPS only, bounded count, no URL supplied by the WebView.

## Refresh policy

- If no complete verified pair exists: download and verify a pair before Xray config validation.
- If a complete pair exists and the last successful check is younger than 24 hours: use it immediately without network I/O.
- If it is 24 hours or older: attempt an update before validation. On update failure, retain and use the existing complete verified pair.
- If no usable pair exists after a failed initial fetch: return a sanitized engine-unavailable/validation error; never expose remote URLs, local paths, raw HTTP details, or Xray stderr to the frontend.

## Xray integration

`engine::xray` owns the geodata service. Before `run -test -config <path>` and before the launch spec is created, `start_ready_profile_inner` asks the service for the verified geodata directory.

The returned directory is passed to Xray through `XRAY_LOCATION_ASSET` in the child environment. This uses Xray's asset-location mechanism and keeps data outside the sidecar directory. `LaunchSpec` gains an engine-owned environment map; only the Xray fallback populates it. The validation child receives the same environment, so validation and launch resolve the exact same files.

The process manager keeps Xray stdout/stderr filtered exactly as it is now. No geodata path, checksum, source URL, raw Xray output, profile content, or subscription data crosses to the WebView.

## Tests and acceptance criteria

Unit tests cover:

1. Source URL and redirect trust boundaries.
2. Exact checksum parsing and rejection of malformed/mismatched entries.
3. Atomic persistence: an incomplete or invalid update cannot replace a valid pair.
4. Refresh decision: initial missing data requires a fetch; a fresh pair avoids network; a stale pair tolerates failed refresh.
5. Xray validation and launch receive the same `XRAY_LOCATION_ASSET` value.

Windows smoke-test acceptance:

1. Start from no local geodata files.
2. Fetch and verify the pair into the user-local data directory.
3. Validate all available ready Xray profiles with the real Xray sidecar.
4. Start and stop the Xray fallback, confirm system proxy cleanup, and repeat once.
5. Confirm `git status` contains only intended source/tests/docs changes and no downloaded `.dat` assets.

## Licensing and packaging

The downloaded data remains user-local runtime content and is not bundled in the repository or desktop installer. The implementation will add an attribution notice naming `Loyalsoldier/v2ray-rules-dat` and preserve its applicable license notice in project documentation/release notes before a production desktop release.
