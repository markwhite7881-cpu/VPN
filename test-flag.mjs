// Quick smoke-test of the flag-detection regex.
const REGIONAL_INDICATOR = "\\uD83C[\\uDDE6-\\uDDFF]";
const re = new RegExp(`(${REGIONAL_INDICATOR}{2})`, "u");
const t = "🇩🇪 DE-Reality-1";
console.log("tag:", t, "len:", t.length);
console.log("regex match:", t.match(re));
console.log("codepoints:", [...t].slice(0, 3).map(c => "U+" + c.codePointAt(0).toString(16)));
