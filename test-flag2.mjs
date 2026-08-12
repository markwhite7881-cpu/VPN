// Re-verify after the rewrite that doesn't use a regex.
const flagFromText = (s) => {
  if (!s) return "";
  for (let i = 0; i + 1 < s.length; ) {
    const hi = s.charCodeAt(i);
    const lo = s.charCodeAt(i + 1);
    if (hi === 0xd83c && lo >= 0xdde6 && lo <= 0xddff) {
      return s.substring(i, i + 2);
    }
    i += hi >= 0xd800 && hi <= 0xdbff ? 2 : 1;
  }
  return "";
};

const cases = [
  "🇩🇪 DE-Reality-1",
  "🇳🇱 NL-Hy2-Edge",
  "🇷🇺 Россия",
  "DE-Reality-1 (no flag)",
  "",
];
for (const c of cases) {
  console.log(JSON.stringify(c), "=>", JSON.stringify(flagFromText(c)));
}
