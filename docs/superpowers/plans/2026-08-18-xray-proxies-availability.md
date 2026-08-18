# Xray PROXIES Availability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Block sing-box-only PROXIES controls while Xray is running and explain the limitation without presenting it as a connection error.

**Architecture:** Keep the existing backend refusal as defense in depth. In `ProxiesCard`, derive an explicit local capability from `Status`; the Xray branch clears stale Clash data, skips polling, renders an unavailable badge and a neutral explanation. A small pure helper keeps the engine boundary unit-testable without adding a DOM test framework.

**Tech Stack:** React 18, TypeScript, Vitest, existing Badge/Card components.

## Global Constraints

- Do not add dependencies or a UI testing framework.
- Do not change the Xray backend, API permissions, provider configuration, or subscription selection flow.
- Do not invoke `list_proxies`, `select_proxy`, or `test_delay` while the active engine is Xray.
- Do not expose raw Xray diagnostics, runtime ports, or provider configuration.
- Keep running sing-box selector, URLTest, latency, and proxy switching behavior unchanged.
- Keep `tsconfig.tsbuildinfo` out of commits.
- Build an unsigned local Windows NSIS installer for manual validation.

---

### Task 1: Block PROXIES when Xray is active

**Files:**
- Modify: `src/components/ProxiesCard.tsx:1-218`
- Create: `src/components/ProxiesCard.test.ts`

**Interfaces:**
- Consumes: `StatusReport` from `src/lib/types.ts`, where `status === "running"` and `engine === "xray"` identify the unavailable state.
- Produces: `proxiesCapability(status: StatusReport): "available" | "xray_unsupported" | "stopped"`, exported only for its focused unit test.

- [ ] **Step 1: Write the failing capability test**

Create `src/components/ProxiesCard.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { proxiesCapability } from "./ProxiesCard";

const runningXray = {
  status: "running",
  pid: 100,
  uptime_secs: 1,
  last_exit_code: null,
  last_error: null,
  engine: "xray",
  profile_key: "profile-1",
  profile_name: "Germany",
} as const;

it("blocks Clash proxy controls for a running Xray connection", () => {
  expect(proxiesCapability(runningXray)).toBe("xray_unsupported");
});
```

- [ ] **Step 2: Run the focused test to verify it fails**

Run:

```powershell
npm test -- --run src/components/ProxiesCard.test.ts
```

Expected: FAIL because `proxiesCapability` is not exported.

- [ ] **Step 3: Implement the minimal capability boundary and blocked UI**

In `src/components/ProxiesCard.tsx`:

```ts
export function proxiesCapability(status: Status) {
  if (status === "running" && status.engine === "xray") return "xray_unsupported";
  return status === "running" ? "available" : "stopped";
}
```

Use the result to:

- short-circuit `refresh`, calling `setData(null)` for `xray_unsupported`;
- prevent the polling interval for Xray;
- render an `unavailable` header badge instead of `live`;
- render a neutral, explanatory message instead of groups or Clash errors:
  `Proxy groups and Clash latency tests are available for sing-box connections. To change an Xray server, select another profile on Home.`

- [ ] **Step 4: Run only the focused frontend checks**

Run:

```powershell
npm test -- --run src/components/ProxiesCard.test.ts
npm run build
```

Expected: focused test passes and TypeScript/Vite production build exits 0.

- [ ] **Step 5: Commit the scoped change**

```powershell
git add src/components/ProxiesCard.tsx src/components/ProxiesCard.test.ts
git commit -m "fix: block proxies controls for Xray"
```

- [ ] **Step 6: Build the manual-test installer**

Run with the established explicit Rust environment and an empty test-manifest flag:

```powershell
$env:PATH = 'C:\Users\Алексей\.cargo\bin;' + $env:PATH
$env:RUSTUP_HOME = 'C:\Users\Public\cwdev\rustup-home'
$env:CARGO_TARGET_DIR = 'C:\Users\Public\cwdev\target'
$env:CLOAKWIRE_TEST_MANIFEST = ''
npm run tauri -- build
```

Verify the NSIS file exists at `C:\Users\Public\cwdev\target\release\bundle\nsis\Cloakwire_1.3.0_x64-setup.exe`, then report its SHA-256 for manual testing.

## Self-review

- The one unit test covers the capability boundary that prevents all Clash calls for Xray.
- The manual NSIS build is the only full runtime validation requested.
- The plan does not add dependencies, expose Xray internals, or alter sing-box behavior.
