/**
 * The library search language (parsed by the server, see `search_syntax.rs`): words plus
 * filters like `#tag`, `in:collection`, `is:fav`. These helpers let the palette suggest
 * values for the filter being typed.
 */

export type FilterHelp = { insert: string; label: string; example: string };

export const FILTERS: FilterHelp[] = [
  { insert: "#", label: "Tag", example: "#pla" },
  { insert: "in:", label: "In a collection", example: "in:desk" },
  { insert: "is:fav ", label: "Favourites", example: "is:fav" },
  { insert: "format:", label: "Model format", example: "format:3mf" },
  { insert: "file:", label: "File name", example: "file:benchy" },
  { insert: "has:image ", label: "Has pictures", example: "has:image  ·  no:image" },
  { insert: "from:", label: "Imported from", example: "from:printables" },
  { insert: "by:", label: "Author", example: "by:prusa" },
  { insert: "-", label: "Exclude", example: "-#gift  ·  -in:garage" },
];

/** Where the last token starts (quotes keep spaces inside a token). */
function lastTokenStart(input: string): number {
  let quoted = false;
  let start = 0;
  for (let i = 0; i < input.length; i++) {
    const c = input[i];
    if (c === '"') quoted = !quoted;
    else if (/\s/.test(c) && !quoted) start = i + 1;
  }
  return start;
}

export type Token = {
  /** The filter being typed (`tag`, `in`, `from`, ...) or null for a plain word. */
  key: string | null;
  /** What's typed after the key, without quotes, lower case. */
  value: string;
  negated: boolean;
  start: number;
};

const ALIASES: Record<string, string> = { tag: "tag", tags: "tag", in: "in", collection: "in", col: "in", is: "is", format: "format", ext: "format", type: "format", from: "from", source: "from", has: "has", no: "has", by: "by", author: "by", file: "file" };

export function currentToken(input: string): Token {
  const start = lastTokenStart(input);
  let raw = input.slice(start);
  const negated = raw.startsWith("-") && raw.length > 1;
  if (negated) raw = raw.slice(1);
  if (raw.startsWith("#")) return { key: "tag", value: raw.slice(1).replace(/"/g, "").toLowerCase(), negated, start };
  const colon = raw.indexOf(":");
  if (colon > 0) {
    const key = ALIASES[raw.slice(0, colon).toLowerCase()];
    if (key) return { key, value: raw.slice(colon + 1).replace(/"/g, "").toLowerCase(), negated, start };
  }
  return { key: null, value: raw.replace(/"/g, "").toLowerCase(), negated: false, start };
}

/** `name`, quoted when it has spaces. */
export function quote(name: string): string {
  return /\s/.test(name) ? `"${name}"` : name;
}

/** Replaces the token being typed with `replacement` and adds a space after it. */
export function replaceToken(input: string, token: Token, replacement: string): string {
  return `${input.slice(0, token.start)}${token.negated ? "-" : ""}${replacement} `;
}

const FILTER_TOKEN = /^-?(#|(tag|tags|in|collection|col|is|format|ext|type|from|source|has|no|by|author|file|filename|files):)/i;

/** Whether the query has words to rank by relevance (filters alone keep the chosen sort). */
export function hasFreeWords(query: string): boolean {
  const tokens = query.match(/(?:[^\s"]+|"[^"]*")+/g) ?? [];
  return tokens.some((t) => !FILTER_TOKEN.test(t));
}
