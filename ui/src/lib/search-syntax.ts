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
  { insert: "type:", label: "Search for", example: "type:tag  ·  type:collection" },
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

const ALIASES: Record<string, string> = { tag: "tag", tags: "tag", in: "in", collection: "in", col: "in", is: "is", format: "format", ext: "format", type: "type", from: "from", source: "from", has: "has", no: "has", by: "by", author: "by", file: "file" };

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

/**
 * Splits the query into tokens the same way the server does (`search_syntax.rs::tokens`):
 * whitespace separates, and quotes keep the space inside a token (quotes stay in the raw
 * token here, so they can be rebuilt verbatim).
 */
function tokenize(query: string): string[] {
  const out: string[] = [];
  let raw = "";
  let quoted = false;
  for (const c of query) {
    if (c === '"') {
      quoted = !quoted;
      raw += c;
    } else if (/\s/.test(c) && !quoted) {
      if (raw) {
        out.push(raw);
        raw = "";
      }
    } else {
      raw += c;
    }
  }
  if (raw) out.push(raw);
  return out;
}

/** Quotes removed and lower-cased, so a token matches whatever case the user typed. */
const tokenKey = (raw: string) => raw.replace(/"/g, "").toLowerCase();

/** Whether `query` contains `token` as a whole token, un-negated and case-insensitively. */
export function hasToken(query: string, token: string): boolean {
  const target = tokenKey(token);
  if (!target) return false;
  return tokenize(query).some((raw) => !raw.startsWith("-") && tokenKey(raw) === target);
}

/** Adds `token` to `query` if absent, removes it if present; whitespace stays tidy. */
export function toggleToken(query: string, token: string): string {
  const tokens = tokenize(query);
  const target = tokenKey(token);
  const idx = tokens.findIndex((raw) => !raw.startsWith("-") && tokenKey(raw) === target);
  if (idx >= 0) tokens.splice(idx, 1);
  else tokens.push(token);
  return tokens.join(" ");
}

export type SearchType = "project" | "tag" | "collection";

/** The kind the palette should search: the last `type:` token, `project` when absent. */
export function searchType(query: string): SearchType {
  let kind: SearchType = "project";
  for (const raw of tokenize(query)) {
    const body = raw.startsWith("-") ? raw.slice(1) : raw;
    const colon = body.indexOf(":");
    if (colon <= 0 || body.slice(0, colon).toLowerCase() !== "type") continue;
    const value = body.slice(colon + 1).replace(/"/g, "").toLowerCase();
    if (value === "tag" || value === "tags") kind = "tag";
    else if (value === "collection" || value === "collections") kind = "collection";
    else if (value === "project" || value === "projects") kind = "project";
  }
  return kind;
}

/** The query with any `type:` token removed, for filtering tags/collections by the rest. */
export function stripType(query: string): string {
  return tokenize(query)
    .filter((raw) => {
      const body = raw.startsWith("-") ? raw.slice(1) : raw;
      const colon = body.indexOf(":");
      return !(colon > 0 && body.slice(0, colon).toLowerCase() === "type");
    })
    .join(" ");
}
