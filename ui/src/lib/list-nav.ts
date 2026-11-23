/**
 * The list a project was opened from (library query or collection page) and its ordered ids,
 * so the project page can go back (`u`) and step to the next/previous project (`j` / `k`).
 */
const KEY = "lima.list-nav";

export type ListNav = { url: string; ids: string[] };

export function rememberList(url: string, ids: string[]) {
  try {
    sessionStorage.setItem(KEY, JSON.stringify({ url, ids } satisfies ListNav));
  } catch {
    // Storage blocked: `u` falls back to the library and `j`/`k` do nothing.
  }
}

export function lastList(): ListNav | null {
  try {
    const raw = sessionStorage.getItem(KEY);
    const v = raw ? (JSON.parse(raw) as ListNav) : null;
    return v && typeof v.url === "string" && Array.isArray(v.ids) ? v : null;
  } catch {
    return null;
  }
}
