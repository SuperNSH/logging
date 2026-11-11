/** What a masked value becomes. */
const MASK = "[REDACTED]";

/** The shortest a word must be before it counts as a random-looking token. */
const RANDOM_LOOKING_MIN_CHARS = 20;

/** A half-open range of code point positions. */
type Range = [number, number];

/**
 * Masks what must not reach a file, before anything is written: the values of the listed keys
 * (case-insensitive), simple email addresses, and long random-looking tokens such as bearer
 * credentials. Everything else passes through unchanged.
 */
export function redact(input: string, keys: readonly string[]): string {
  const wanted = keys.map((key) => key.trim().toLowerCase()).filter((key) => key !== "");
  const characters = Array.from(input);
  const words = wordRanges(characters);
  const masked: Range[] = [];
  maskKeyValues(characters, words, wanted, masked);
  maskByShape(characters, words, masked);
  // The two passes push in discovery order, not position order: sort before splicing.
  masked.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  return rebuild(characters, masked);
}

/** The words of `characters`: maximal runs that neither whitespace nor value punctuation splits. */
function wordRanges(characters: readonly string[]): Range[] {
  const ranges: Range[] = [];
  let start: number | null = null;
  for (const [index, character] of characters.entries()) {
    if (isWordChar(character)) {
      if (start === null) start = index;
    } else if (start !== null) {
      ranges.push([start, index]);
      start = null;
    }
  }
  if (start !== null) ranges.push([start, characters.length]);
  return ranges;
}

/** The punctuation that surrounds values and so ends a word; `@` and `.` stay inside one. */
const VALUE_PUNCTUATION = ",;:\"'()[]{}<>=";

function isWordChar(character: string): boolean {
  return !/\s/.test(character) && !VALUE_PUNCTUATION.includes(character);
}

/** Masks the value word after each listed key, where a colon or an equals introduces it. */
function maskKeyValues(
  characters: readonly string[],
  words: readonly Range[],
  keys: readonly string[],
  masked: Range[],
): void {
  for (let position = 0; position < words.length; position += 1) {
    const range = words[position];
    if (range === undefined) continue;
    const [start, end] = range;
    const key = characters.slice(start, end).join("").toLowerCase();
    if (!keys.includes(key)) continue;
    const next = words[position + 1];
    if (next === undefined) continue;
    const [valueStart, valueEnd] = next;
    // Only a run of colons or equals (spaces and quotes aside) makes the next word a value.
    const separator = characters
      .slice(end, valueStart)
      .join("")
      .split("")
      .filter((character) => character !== " " && character !== '"' && character !== "'")
      .join("");
    if (!/^[:=]+$/.test(separator)) continue;
    if (!masked.some(([from]) => from === valueStart)) masked.push([valueStart, valueEnd]);
  }
}

/** Masks what gives itself away by shape: an email address, or a long token mixing letters and digits. */
function maskByShape(characters: readonly string[], words: readonly Range[], masked: Range[]): void {
  for (const range of words) {
    const [start, end] = range;
    if (masked.some(([from, until]) => from <= start && start < until)) continue;
    const word = characters.slice(start, end).join("");
    if (isEmail(word) || isRandomLooking(word)) masked.push([start, end]);
  }
}

/** A simple email address: one `@`, a dotted domain, and an all-letter TLD of two or more. */
function isEmail(word: string): boolean {
  const at = word.indexOf("@");
  if (at < 1) return false;
  const domain = word.slice(at + 1);
  if (domain.includes("@")) return false;
  const dot = domain.lastIndexOf(".");
  if (dot === -1) return false;
  const tld = Array.from(domain.slice(dot + 1));
  return tld.length >= 2 && tld.every((character) => /\p{L}/u.test(character));
}

function isRandomLooking(word: string): boolean {
  const characters = Array.from(word);
  return (
    characters.length >= RANDOM_LOOKING_MIN_CHARS &&
    characters.some((character) => character >= "0" && character <= "9") &&
    characters.some((character) => /\p{L}/u.test(character))
  );
}

/** Splices `MASK` over the masked ranges, sorted by position, keeping every other character. */
function rebuild(characters: readonly string[], masked: readonly Range[]): string {
  let output = "";
  let copied = 0;
  for (const [start, end] of masked) {
    if (start < copied || start >= end) continue;
    output += characters.slice(copied, start).join("");
    output += MASK;
    copied = end;
  }
  return output + characters.slice(copied).join("");
}
