// Detect emoji country flags (regional indicator pairs) inside a
// string. Used to extract the flag from profile tags like
// "🇩🇪 DE-Reality-1" so we can render it next to the row.

/** Extract the first flag emoji from a free-form string, or "" if none. */
export function flagFromText(s: string): string {
  if (!s) return "";
  // Each flag is two Regional Indicator Symbols, encoded in UTF-16
  // as 2-char surrogate pairs each (high=0xD83C, low ∈ [0xDDE6, 0xDDFF]).
  for (let i = 0; i + 3 < s.length; ) {
    const hi1 = s.charCodeAt(i);
    const lo1 = s.charCodeAt(i + 1);
    if (hi1 !== 0xd83c || lo1 < 0xdde6 || lo1 > 0xddff) {
      i += hi1 >= 0xd800 && hi1 <= 0xdbff ? 2 : 1;
      continue;
    }
    const hi2 = s.charCodeAt(i + 2);
    const lo2 = s.charCodeAt(i + 3);
    if (hi2 === 0xd83c && lo2 >= 0xdde6 && lo2 <= 0xddff) {
      return s.substring(i, i + 4);
    }
    // Lone RI — skip and keep scanning.
    i += 2;
  }
  return "";
}

/**
 * Best-effort country code inference for a profile, in order:
 *   1. flag emoji at the start of the tag (e.g. "🇩🇪 DE-…")
 *   2. Russian country name inside the tag (e.g. "Германия …", "🇩🇪" wasn't rendered → fall through here)
 *   3. English country name inside the tag ("Germany …")
 *   4. TLD suffix on the server hostname (e.g. ".de" → DE)
 *   5. Sub-domain prefix hint (e.g. "nl-1.foo.bar" → NL)
 *   6. Caller-supplied GeoIP map (`opts.geoipByIp[server]` for IPs)
 *   7. Fall back to "??" — the UI renders a small "··" badge.
 *
 * The Russian-name map is the workhorse in practice: subscription
 * providers ship tags like "🇷🇺 Россия" or "DE Белые списки", and
 * the emoji on the front doesn't survive every user's font setup
 * (notably WebView2 on Windows renders "🇷🇺" as the bare two-letter
 * code "RU" — see `flagFromText`'s `RI` failure mode). Matching the
 * Cyrillic / Latin country name is the bit that actually saves us.
 */
export function flagForProfile(opts: {
  tag?: string;
  server?: string;
  /** Optional IP → country-code map (from useGeoIp). Only checked
   *  if `server` is a public IPv4 address and steps 1–5 missed. */
  geoipByIp?: Record<string, string>;
}): { flag: string; code: string } {
  const fromTag = flagFromText(opts.tag ?? "");
  if (fromTag) {
    return { flag: fromTag, code: flagToCode(fromTag) };
  }
  const tagText = (opts.tag ?? "").toLowerCase();
  for (const [needle, code] of COUNTRY_NAME_TO_CODE) {
    if (tagText.includes(needle)) {
      return { flag: codeToFlag(code), code };
    }
  }
  const SERVER_TLD_TO_CODE: Record<string, string> = {
    ru: "RU",
    de: "DE",
    nl: "NL",
    fr: "FR",
    gb: "GB",
    uk: "GB",
    us: "US",
    sg: "SG",
    jp: "JP",
    hk: "HK",
    tr: "TR",
    pl: "PL",
    fi: "FI",
    se: "SE",
    ca: "CA",
    au: "AU",
    ch: "CH",
    it: "IT",
    es: "ES",
    ua: "UA",
  };
  const host = (opts.server ?? "").toLowerCase();
  // TLD — e.g. "de-1.example.de" → "de" → DE
  const tld = host.split(".").pop() ?? "";
  const tldCode = SERVER_TLD_TO_CODE[tld];
  if (tldCode) {
    return { flag: codeToFlag(tldCode), code: tldCode };
  }
  // Sub-domain prefix — e.g. "nl-1.foo.bar" → first label "nl-1" →
  // "nl" → NL. This is a weak signal (any 2-letter prefix works)
  // but it covers common provider hostnames that don't have a
  // country TLD.
  const firstLabel = host.split(".")[0] ?? "";
  if (/^[a-z]{2}-?\d*$/.test(firstLabel)) {
    const code = firstLabel.slice(0, 2).toUpperCase();
    if (
      code === "DE" ||
      code === "RU" ||
      code === "NL" ||
      code === "FR" ||
      code === "GB" ||
      code === "US" ||
      code === "JP" ||
      code === "TR" ||
      code === "PL" ||
      code === "IT" ||
      code === "ES" ||
      code === "KZ" ||
      code === "AE" ||
      code === "SG" ||
      code === "IN" ||
      code === "FI" ||
      code === "SE" ||
      code === "CA" ||
      code === "AT" ||
      code === "CH" ||
      code === "UA"
    ) {
      return { flag: codeToFlag(code), code };
    }
  }
  // GeoIP fallback — the caller (useGeoIp) has previously asked
  // ip-api.com where the server's IP sits, cached the answer in
  // localStorage, and handed us the map. If the IP is in there,
  // use it; otherwise stay at "??" until the next render after
  // the network call resolves.
  if (opts.geoipByIp) {
    const geo = opts.geoipByIp[opts.server ?? ""];
    if (geo && geo.length === 2) {
      return { flag: codeToFlag(geo), code: geo };
    }
  }
  return { flag: "🌐", code: "??" };
}

/**
 * Country-name → ISO code map, kept as an array of `[needle, code]`
 * tuples so we can iterate it with `String.prototype.includes`.
 * Order matters when one name is a substring of another — longer /
 * more specific entries come first (e.g. "Великобритания" before
 * "Британия"; "Белые списки" must NOT be matched by "Бел", which is
 * why we deliberately use whole words / discriminative fragments
 * rather than 2-letter stems).
 */
const COUNTRY_NAME_TO_CODE: ReadonlyArray<readonly [string, string]> = [
  // English — common in international provider tags.
  ["united kingdom", "GB"],
  ["great britain", "GB"],
  ["england", "GB"],
  ["netherlands", "NL"],
  ["holland", "NL"],
  ["germany", "DE"],
  ["france", "FR"],
  ["spain", "ES"],
  ["italy", "IT"],
  ["poland", "PL"],
  ["sweden", "SE"],
  ["finland", "FI"],
  ["norway", "NO"],
  ["denmark", "DK"],
  ["ireland", "IE"],
  ["belgium", "BE"],
  ["portugal", "PT"],
  ["greece", "GR"],
  ["czech", "CZ"],
  ["hungary", "HU"],
  ["slovakia", "SK"],
  ["romania", "RO"],
  ["bulgaria", "BG"],
  ["croatia", "HR"],
  ["slovenia", "SI"],
  ["estonia", "EE"],
  ["latvia", "LV"],
  ["lithuania", "LT"],
  ["ukraine", "UA"],
  ["belarus", "BY"],
  ["israel", "IL"],
  ["saudi arabia", "SA"],
  ["emirates", "AE"],
  ["egypt", "EG"],
  ["south africa", "ZA"],
  ["mexico", "MX"],
  ["argentina", "AR"],
  ["chile", "CL"],
  ["colombia", "CO"],
  ["brazil", "BR"],
  ["japan", "JP"],
  ["korea", "KR"],
  ["china", "CN"],
  ["taiwan", "TW"],
  ["hong kong", "HK"],
  ["thailand", "TH"],
  ["malaysia", "MY"],
  ["indonesia", "ID"],
  ["philippines", "PH"],
  ["vietnam", "VN"],
  ["singapore", "SG"],
  ["india", "IN"],
  ["turkey", "TR"],
  ["turkey", "TR"],
  ["canada", "CA"],
  ["australia", "AU"],
  ["switzerland", "CH"],
  ["austria", "AT"],
  // Russian — used by the subscription provider we test against.
  ["великобритания", "GB"],
  ["англия", "GB"],
  ["нидерланды", "NL"],
  ["голландия", "NL"],
  ["германия", "DE"],
  ["франция", "FR"],
  ["испания", "ES"],
  ["италия", "IT"],
  ["польша", "PL"],
  ["швеция", "SE"],
  ["финляндия", "FI"],
  ["норвегия", "NO"],
  ["дания", "DK"],
  ["ирландия", "IE"],
  ["бельгия", "BE"],
  ["португалия", "PT"],
  ["греция", "GR"],
  ["чехия", "CZ"],
  ["венгрия", "HU"],
  ["словакия", "SK"],
  ["румыния", "RO"],
  ["болгария", "BG"],
  ["хорватия", "HR"],
  ["словения", "SI"],
  ["эстония", "EE"],
  ["латвия", "LV"],
  ["литва", "LT"],
  ["украина", "UA"],
  ["беларусь", "BY"],
  ["израиль", "IL"],
  ["саудовская", "SA"],
  ["эмираты", "AE"],
  ["египет", "EG"],
  ["мексика", "MX"],
  ["аргентина", "AR"],
  ["чили", "CL"],
  ["колумбия", "CO"],
  ["бразилия", "BR"],
  ["япония", "JP"],
  ["корея", "KR"],
  ["китай", "CN"],
  ["тайвань", "TW"],
  ["гонконг", "HK"],
  ["таиланд", "TH"],
  ["малайзия", "MY"],
  ["индонезия", "ID"],
  ["филиппины", "PH"],
  ["вьетнам", "VN"],
  ["сингапур", "SG"],
  ["индия", "IN"],
  ["турция", "TR"],
  ["канада", "CA"],
  ["австралия", "AU"],
  ["швейцария", "CH"],
  ["австрия", "AT"],
  ["казахстан", "KZ"],
  ["узбекистан", "UZ"],
  ["армения", "AM"],
  ["грузия", "GE"],
  ["азербайджан", "AZ"],
  ["молдова", "MD"],
  ["таджикистан", "TJ"],
  ["киргизия", "KG"],
  ["сербия", "RS"],
  ["черногория", "ME"],
  ["македония", "MK"],
  ["албания", "AL"],
  ["боcния", "BA"],
  ["исландия", "IS"],
  ["люксембург", "LU"],
  ["монако", "MC"],
  ["мальта", "MT"],
  ["кипр", "CY"],
  ["россия", "RU"],
  ["сша", "US"],
  ["америка", "US"],
];

/** Convert a country code like "DE" into the flag emoji "🇩🇪". */
export function codeToFlag(code: string): string {
  if (!code || code.length !== 2) return "🌐";
  const A = 0x1f1e6;
  const codeA = "A".charCodeAt(0);
  const c1 = code.toUpperCase().charCodeAt(0) - codeA + A;
  const c2 = code.toUpperCase().charCodeAt(1) - codeA + A;
  return String.fromCodePoint(c1, c2);
}

/** Convert a flag emoji back into a two-letter country code, e.g. "🇩🇪" → "DE". */
export function flagToCode(flag: string): string {
  if (!flag || flag.length < 2) return "??";
  // Regional Indicator letters live in the supplementary plane
  // (0x1F1E6–0x1F1FF), so each glyph is a UTF-16 surrogate pair.
  // `charCodeAt(0)` would return the high surrogate (≈0xD83C), not
  // the code point we need — `codePointAt` is the only way to read
  // the actual codepoint, and we have to skip 2 UTF-16 code units
  // (one surrogate pair) to land on the second flag letter.
  const cp1 = flag.codePointAt(0) ?? 0;
  const cp2 = flag.codePointAt(2) ?? 0;
  if (cp1 < 0x1f1e6 || cp1 > 0x1f1ff || cp2 < 0x1f1e6 || cp2 > 0x1f1ff) {
    return "??";
  }
  const first = cp1 - 0x1f1e6 + 65;
  const second = cp2 - 0x1f1e6 + 65;
  if (first < 65 || first > 90 || second < 65 || second > 90) return "??";
  return String.fromCharCode(first, second);
}
