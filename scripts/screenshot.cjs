// Capture screenshots of the running Vite dev server (UI preview).
//
// Run with:  node scripts/screenshot.cjs
//   URL defaults to http://localhost:1420 (set URL= to override).
const path = require("path");
const fs = require("fs");
const { chromium } = require("playwright");
process.env.PLAYWRIGHT_BROWSERS_PATH =
  process.env.PLAYWRIGHT_BROWSERS_PATH ||
  "C:\\Users\\Алексей\\AppData\\Local\\ms-playwright";

const outDir =
  "C:\\Users\\Алексей\\.minimax-agent\\projects\\singbox-client\\screenshots";

async function shot(page, name) {
  const out = path.join(outDir, name);
  await page.screenshot({ path: out, fullPage: true });
  console.log("Saved", out);
}

async function clickTab(page, label) {
  const btn = await page.$(`button[role="tab"]:has-text("${label}")`);
  if (btn) {
    await btn.click();
    await page.waitForTimeout(400);
  }
}

(async () => {
  const browser = await chromium.launch({ headless: true });
  const ctx = await browser.newContext({
    viewport: { width: 1320, height: 1000 },
    deviceScaleFactor: 1.4,
    colorScheme: "dark",
  });
  const page = await ctx.newPage();

  const url = process.env.URL || "http://localhost:1420/";
  console.log("Navigating to", url);
  await page.goto(url, { waitUntil: "domcontentloaded" });
  await page.waitForTimeout(2500);

  fs.mkdirSync(outDir, { recursive: true });

  // Clear localStorage so we always start on the Home tab.
  await page.evaluate(() => window.localStorage.clear());
  await page.reload({ waitUntil: "domcontentloaded" });
  await page.waitForTimeout(1500);

  // 01: Home tab — the new minimal landing.
  await shot(page, "01-home.png");

  // 02: Servers tab — list of profiles.
  await clickTab(page, "Servers");
  await page.waitForTimeout(500);
  await shot(page, "02-servers.png");

  // 03: Subscriptions tab — empty state (no subs in preview mode).
  await clickTab(page, "Subscriptions");
  await page.waitForTimeout(500);
  await shot(page, "03-subscriptions.png");

  // 04: Config tab — config builder + binary info.
  await clickTab(page, "Config");
  await page.waitForTimeout(500);
  await shot(page, "04-config.png");

  // 05: Logs tab.
  await clickTab(page, "Logs");
  await page.waitForTimeout(500);
  await shot(page, "05-logs.png");

  // 06: Home tab in "running" state — flip Stopped → Connected in the
  // DOM so the hero card shows the live state without spinning up a
  // real sing-box process. Then take a final hero shot.
  await clickTab(page, "Home");
  await page.waitForTimeout(300);
  await page.evaluate(() => {
    const h = Array.from(document.querySelectorAll("h1")).find((n) =>
      n.textContent.includes("Disconnected"),
    );
    if (h) h.textContent = "Connected";
    const sub = Array.from(document.querySelectorAll("p")).find((n) =>
      n.textContent.includes("Add a server in the Servers tab"),
    );
    if (sub) sub.textContent = "via DE-Reality-1";
    // Replace the Connect button with a Disconnect one.
    const btn = Array.from(document.querySelectorAll("button")).find((n) =>
      n.textContent.trim() === "Connect",
    );
    if (btn) {
      btn.textContent = "Disconnect";
      btn.classList.remove("from-black", "to-zinc-700");
    }
  });
  await page.waitForTimeout(300);
  await shot(page, "06-home-connected.png");

  await browser.close();
})().catch((e) => {
  console.error("FAIL:", e);
  process.exit(1);
});
