import assert from 'node:assert/strict';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import ts from 'typescript';

const sourceUrl = new URL('../src/mobile/lib/mobileUi.ts', import.meta.url);
const homeScreenUrl = new URL('../src/mobile/screens/HomeScreen.tsx', import.meta.url);
const serversScreenUrl = new URL('../src/mobile/screens/ServersScreen.tsx', import.meta.url);
const mobileAppUrl = new URL('../src/mobile/MobileApp.tsx', import.meta.url);
const settingsScreenUrl = new URL('../src/mobile/screens/SettingsScreen.tsx', import.meta.url);
const logsScreenUrl = new URL('../src/mobile/screens/LogsScreen.tsx', import.meta.url);
const stylesUrl = new URL('../src/index.css', import.meta.url);
const outputUrl = new URL('../.mobile-ui-check/mobileUi.mjs', import.meta.url);

const source = await readFile(sourceUrl, 'utf8');
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
}).outputText;
await mkdir(new URL('../.mobile-ui-check/', import.meta.url), { recursive: true });
await writeFile(outputUrl, compiled, 'utf8');

const {
  orderAppsForPicker,
  summarizeSelectedApps,
  classifySourceInput,
  latencyTone,
  swipeDirection,
  adjacentTabIndex,
  summarizeRoutingPolicy,
} = await import(`${outputUrl.href}?check=${Date.now()}`);

const apps = [
  { label: 'Chrome', packageName: 'com.android.chrome', hasInternet: true, system: false },
  { label: 'Telegram', packageName: 'org.telegram.messenger', hasInternet: true, system: false },
  { label: 'Clock', packageName: 'com.android.deskclock', hasInternet: false, system: true },
];

assert.deepEqual(
  orderAppsForPicker(apps, new Set(['org.telegram.messenger']), 'include').map((app) => app.packageName),
  ['org.telegram.messenger', 'com.android.chrome', 'com.android.deskclock'],
);
assert.deepEqual(
  orderAppsForPicker(apps, new Set(['org.telegram.messenger']), 'exclude').map((app) => app.packageName),
  ['org.telegram.messenger', 'com.android.chrome', 'com.android.deskclock'],
);
assert.deepEqual(
  orderAppsForPicker(apps, new Set(['org.telegram.messenger']), 'all').map((app) => app.packageName),
  ['com.android.chrome', 'org.telegram.messenger', 'com.android.deskclock'],
);
assert.deepEqual(apps.map((app) => app.packageName), [
  'com.android.chrome',
  'org.telegram.messenger',
  'com.android.deskclock',
]);
assert.equal(summarizeSelectedApps(apps, []), 'No apps selected');
assert.equal(summarizeSelectedApps(apps, ['org.telegram.messenger']), 'Telegram');
assert.equal(
  summarizeSelectedApps(apps, ['org.telegram.messenger', 'com.android.chrome']),
  'Telegram, Chrome',
);
assert.equal(
  summarizeSelectedApps(apps, ['org.telegram.messenger', 'com.android.chrome', 'missing.app']),
  'Telegram, Chrome +1',
);
assert.equal(
  summarizeSelectedApps(apps, ['missing.one', 'missing.two']),
  '2 apps selected',
);
assert.equal(classifySourceInput('  ').kind, 'empty');
assert.equal(classifySourceInput('vless://example').kind, 'share');
assert.equal(classifySourceInput('HY2://example').kind, 'share');
assert.equal(classifySourceInput('https://provider.example/sub').kind, 'subscription');
assert.equal(classifySourceInput('http://provider.example').kind, 'subscription');
assert.equal(classifySourceInput('ftp://provider.example').kind, 'invalid');
assert.equal(classifySourceInput('provider.example').kind, 'invalid');
assert.equal(classifySourceInput('\n  \nHY2://example\nhttps://provider.example').kind, 'share');
assert.equal(classifySourceInput('\n  \nhttps://provider.example\nvless://example').kind, 'subscription');
assert.equal(latencyTone(undefined), 'pending');
assert.equal(latencyTone(249), 'fast');
assert.equal(latencyTone(250), 'medium');
assert.equal(latencyTone(699), 'medium');
assert.equal(latencyTone(700), 'slow');
assert.equal(swipeDirection({ dx: -80, dy: 20, startTarget: null }), 'next');
assert.equal(swipeDirection({ dx: 80, dy: 20, startTarget: {} }), 'previous');
assert.equal(swipeDirection({ dx: 72, dy: 20, startTarget: null }), 'previous');
assert.equal(swipeDirection({ dx: -60, dy: 5, startTarget: null }), null);
assert.equal(swipeDirection({ dx: -80, dy: 80, startTarget: null }), null);
assert.equal(adjacentTabIndex(0, 'previous', 5), 0);
assert.equal(adjacentTabIndex(0, 'next', 5), 1);
assert.equal(adjacentTabIndex(2, 'previous', 5), 1);
assert.equal(adjacentTabIndex(4, 'next', 5), 4);
assert.equal(summarizeRoutingPolicy('proxy', 'all', []), 'Global · All apps');
assert.equal(summarizeRoutingPolicy('proxy', 'exclude', []), 'Global · All apps');
assert.equal(summarizeRoutingPolicy('auto', 'include', ['org.telegram.messenger']), 'Auto · 1 app selected');
assert.equal(summarizeRoutingPolicy('direct', 'exclude', ['a', 'b']), 'Direct · All except 2 apps');

const hasOwnElement = Object.prototype.hasOwnProperty.call(globalThis, 'Element');
const hasOwnWindow = Object.prototype.hasOwnProperty.call(globalThis, 'window');
const originalElement = globalThis.Element;
const originalWindow = globalThis.window;

class ElementShim {
  constructor({ parentElement = null, selector = '', editable = false, scrollHeight = 0, clientHeight = 0 }) {
    this.parentElement = parentElement;
    this.selector = selector;
    this.editable = editable;
    this.scrollHeight = scrollHeight;
    this.clientHeight = clientHeight;
    this.scrollWidth = 0;
    this.clientWidth = 0;
  }

  matches(selector) {
    return selector.split(', ').includes(this.selector);
  }

  getAttribute(name) {
    return name === 'contenteditable' && this.editable ? '' : null;
  }
}

try {
  globalThis.Element = ElementShim;
  globalThis.window = {
    getComputedStyle: () => ({ overflow: 'visible', overflowX: 'visible', overflowY: 'visible' }),
  };
  assert.equal(
    swipeDirection({ dx: -80, dy: 20, startTarget: new ElementShim({ selector: 'button' }) }),
    null,
  );
  assert.equal(
    swipeDirection({
      dx: -80,
      dy: 20,
      startTarget: new ElementShim({ selector: 'button' }),
      allowControls: true,
    }),
    'next',
  );
  assert.equal(
    swipeDirection({
      dx: -80,
      dy: 20,
      startTarget: new ElementShim({ selector: 'input' }),
      allowControls: true,
    }),
    null,
  );
  assert.equal(
    swipeDirection({ dx: -80, dy: 20, startTarget: new ElementShim({ editable: true }) }),
    null,
  );

  const settingsScrollContainer = new ElementShim({ scrollHeight: 400, clientHeight: 100 });
  globalThis.window = {
    getComputedStyle: (element) => element === settingsScrollContainer
      ? { overflow: 'auto', overflowX: 'auto', overflowY: 'auto' }
      : { overflow: 'visible', overflowX: 'visible', overflowY: 'visible' },
  };
  assert.equal(
    swipeDirection({
      dx: -80,
      dy: 20,
      startTarget: new ElementShim({ parentElement: settingsScrollContainer, selector: 'button' }),
      allowControls: true,
      scrollContainer: settingsScrollContainer,
    }),
    'next',
  );

  const scrollableParent = new ElementShim({ scrollHeight: 200, clientHeight: 100 });
  globalThis.window = {
    getComputedStyle: (element) => element === scrollableParent
      ? { overflow: 'auto', overflowX: 'auto', overflowY: 'auto' }
      : { overflow: 'visible', overflowX: 'visible', overflowY: 'visible' },
  };
  assert.equal(
    swipeDirection({
      dx: -80,
      dy: 20,
      startTarget: new ElementShim({ parentElement: scrollableParent }),
      allowControls: true,
      scrollContainer: settingsScrollContainer,
    }),
    null,
  );
} finally {
  if (hasOwnElement) globalThis.Element = originalElement;
  else delete globalThis.Element;

  if (hasOwnWindow) globalThis.window = originalWindow;
  else delete globalThis.window;
}

const homeScreenSource = await readFile(homeScreenUrl, 'utf8');
const serversScreenSource = await readFile(serversScreenUrl, 'utf8');
const mobileAppSource = await readFile(mobileAppUrl, 'utf8');
const settingsScreenSource = await readFile(settingsScreenUrl, 'utf8');
const logsScreenSource = await readFile(logsScreenUrl, 'utf8');
const stylesSource = await readFile(stylesUrl, 'utf8');

assert.match(homeScreenSource, /<button[\s\S]*?onClick=\{onOpenServers\}[\s\S]*?\{serverLine\}/);
assert.match(homeScreenSource, /<button[\s\S]*?onClick=\{onOpenRouting\}[\s\S]*?\{routingSummary\}/);
assert.doesNotMatch(homeScreenSource, /Check connection/);
assert.match(serversScreenSource, /import \{ latencyTone \} from "\.\.\/lib\/mobileUi"/);
assert.match(serversScreenSource, /const tone = latencyTone\(ms\);/);
assert.match(serversScreenSource, /aria-label=\{dim \? "Latency unavailable after ping" : "Latency not measured"\}/);
assert.match(serversScreenSource, /aria-label=\{`Latency \$\{ms\} milliseconds, \$\{tone\}`\}/);

assert.doesNotMatch(mobileAppSource, /\{ id: "logs", label: "Logs"/);
assert.match(mobileAppSource, /onOpenLogs=\{\(\) => \{[\s\S]*?setLogsOpen\(true\)/);
assert.match(mobileAppSource, /<LogsScreen[\s\S]*?onBack=\{\(\) => \{[\s\S]*?setLogsOpen\(false\)/);
assert.match(settingsScreenSource, /onOpenLogs: \(\) => void/);
assert.match(settingsScreenSource, /onClick=\{onOpenLogs\}/);
assert.match(logsScreenSource, /onBack: \(\) => void/);
assert.match(logsScreenSource, /onClick=\{onBack\}/);
assert.match(mobileAppSource, /mobile-view-enter-\$\{transitionDirection\}/);
assert.match(stylesSource, /\.mobile-view-enter-next/);
assert.match(stylesSource, /\.mobile-view-enter-previous/);
assert.match(stylesSource, /@media \(prefers-reduced-motion: reduce\)/);

console.log('mobile UI helper checks passed');
