const flagFromText = (s) => {
  if (!s) return "";
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
    i += 2;
  }
  return "";
};
for (const c of ["🇩🇪 DE", "🇳🇱 NL", "🇷🇺 RU", "no flag here", ""]) {
  console.log(JSON.stringify(c), "=>", JSON.stringify(flagFromText(c)));
}
