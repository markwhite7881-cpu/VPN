import assert from "node:assert/strict";
import { isValidProfileSelection } from "../src/lib/profileSelection.ts";

assert.equal(
  isValidProfileSelection(-1, 3),
  true,
  "Auto (-1) must remain a valid selection while profiles exist",
);
assert.equal(isValidProfileSelection(0, 3), true);
assert.equal(isValidProfileSelection(2, 3), true);
assert.equal(isValidProfileSelection(3, 3), false);
assert.equal(isValidProfileSelection(-2, 3), false);
assert.equal(
  isValidProfileSelection(-1, 0),
  true,
  "Auto (-1) must remain valid after every profile is removed",
);
assert.equal(isValidProfileSelection(0, 0), false);

console.log("PASS: profile selection normalization preserves Auto mode.");
