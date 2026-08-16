export type PickerMode = 'all' | 'include' | 'exclude';

export interface AppEntry {
  label: string;
  packageName: string;
  hasInternet: boolean;
  system: boolean;
}

export interface SourceInputClassification {
  kind: 'empty' | 'share' | 'subscription' | 'invalid';
  value: string;
}

export interface SwipeInput {
  dx: number;
  dy: number;
  startTarget: EventTarget | null;
  allowControls?: boolean;
  scrollContainer?: Element | null;
}

const SHARE_SCHEMES = new Set(['vless', 'vmess', 'trojan', 'ss', 'hysteria', 'hy2']);
const SWIPE_CONTROL_SELECTOR = [
  'button',
  'a',
  '[role="button"]',
  '[role="checkbox"]',
  '[role="link"]',
  '[role="radio"]',
  '[role="slider"]',
  '[role="switch"]',
  '[role="tab"]',
].join(', ');

const SWIPE_EDITABLE_SELECTOR = [
  'input',
  'textarea',
  'select',
  '[contenteditable]',
  '[role="textbox"]',
].join(', ');

function compareApps(a: AppEntry, b: AppEntry): number {
  return Number(b.hasInternet) - Number(a.hasInternet) || a.label.localeCompare(b.label);
}

export function orderAppsForPicker(
  apps: AppEntry[],
  selectedPackages: ReadonlySet<string>,
  mode: PickerMode,
): AppEntry[] {
  const sorted = [...apps].sort(compareApps);
  if (mode === 'all') return sorted;

  return [
    ...sorted.filter((app) => selectedPackages.has(app.packageName)),
    ...sorted.filter((app) => !selectedPackages.has(app.packageName)),
  ];
}

export function summarizeSelectedApps(apps: AppEntry[] | null, packageNames: string[]): string {
  if (packageNames.length === 0) return 'No apps selected';
  if (!apps) return `${packageNames.length} apps selected`;

  const labelsByPackage = new Map(apps.map((app) => [app.packageName, app.label]));
  const labels = packageNames
    .map((packageName) => labelsByPackage.get(packageName))
    .filter((label): label is string => label !== undefined);
  if (labels.length === 0) return `${packageNames.length} apps selected`;

  const remaining = packageNames.length - Math.min(labels.length, 2);

  return `${labels.slice(0, 2).join(', ')}${remaining > 0 ? ` +${remaining}` : ''}`;
}

export function classifySourceInput(input: string): SourceInputClassification {
  const value = input.trim();
  if (!value) return { kind: 'empty', value };

  const scheme = value.match(/^([a-z][a-z0-9+.-]*):/i)?.[1]?.toLowerCase();
  if (scheme && SHARE_SCHEMES.has(scheme)) return { kind: 'share', value };
  if (scheme === 'http' || scheme === 'https') return { kind: 'subscription', value };

  return { kind: 'invalid', value };
}

export function latencyTone(ms: number | undefined): 'pending' | 'fast' | 'medium' | 'slow' {
  if (ms === undefined) return 'pending';
  if (ms < 250) return 'fast';
  if (ms < 700) return 'medium';
  return 'slow';
}

function isSwipeBlocked(
  startTarget: EventTarget | null,
  allowControls = false,
  scrollContainer: Element | null = null,
): boolean {
  if (!startTarget || typeof Element === 'undefined' || !(startTarget instanceof Element)) {
    return false;
  }

  for (let element: Element | null = startTarget; element; element = element.parentElement) {
    if (
      element.matches(SWIPE_EDITABLE_SELECTOR) ||
      element.getAttribute('contenteditable') !== null ||
      (!allowControls && element.matches(SWIPE_CONTROL_SELECTOR))
    ) {
      return true;
    }

    if (typeof window !== 'undefined') {
      const style = window.getComputedStyle(element);
      const scrollable = /auto|scroll/.test(`${style.overflow}${style.overflowX}${style.overflowY}`);
      if (
        element !== scrollContainer &&
        scrollable &&
        (element.scrollHeight > element.clientHeight || element.scrollWidth > element.clientWidth)
      ) {
        return true;
      }
    }
  }

  return false;
}

export function swipeDirection({
  dx,
  dy,
  startTarget,
  allowControls = false,
  scrollContainer = null,
}: SwipeInput): 'previous' | 'next' | null {
  if (
    Math.abs(dx) < 72 ||
    Math.abs(dx) <= Math.abs(dy) ||
    isSwipeBlocked(startTarget, allowControls, scrollContainer)
  ) {
    return null;
  }

  return dx > 0 ? 'previous' : 'next';
}

export function adjacentTabIndex(
  currentIndex: number,
  direction: 'previous' | 'next',
  tabCount: number,
): number {
  if (tabCount <= 0) return 0;
  const offset = direction === 'previous' ? -1 : 1;
  return Math.min(Math.max(currentIndex + offset, 0), tabCount - 1);
}

export function summarizeRoutingPolicy(
  finalOutbound: 'proxy' | 'auto' | 'direct' | string | undefined,
  appMode: PickerMode | undefined,
  selectedPackages: readonly string[],
): string {
  const modeLabel =
    finalOutbound === 'auto' ? 'Auto' : finalOutbound === 'direct' ? 'Direct' : 'Global';
  if (appMode === 'include') {
    return `${modeLabel} · ${selectedPackages.length} ${selectedPackages.length === 1 ? 'app' : 'apps'} selected`;
  }
  if (appMode === 'exclude' && selectedPackages.length > 0) {
    return `${modeLabel} · All except ${selectedPackages.length} ${selectedPackages.length === 1 ? 'app' : 'apps'}`;
  }
  return `${modeLabel} · All apps`;
}
