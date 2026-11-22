import { useQuery } from "@tanstack/react-query";

import type { components } from "@/gen/openapi";
import { ApiError, apiGet } from "@/lib/api";

export type Tag = components["schemas"]["Tag"];
type ListTagsResponse = components["schemas"]["ListTagsResponse"];

export const TAGS_QUERY_KEY = ["tags"] as const;

/** Fetches every tag (following pagination), sorted by name. */
export async function fetchAllTags(): Promise<Tag[]> {
  const all: Tag[] = [];
  let cursor: string | null = null;
  for (;;) {
    const page: ListTagsResponse = await apiGet<ListTagsResponse>(
      `/tags?limit=200${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ""}`,
    );
    all.push(...page.items);
    if (!page.next_cursor || page.items.length < 200) break;
    cursor = page.next_cursor;
  }
  return all.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
}

export function useAllTags() {
  return useQuery({ queryKey: TAGS_QUERY_KEY, queryFn: fetchAllTags });
}

/** Black or white, whichever reads better on the given `#RRGGBB` background. */
export function tagTextColor(hex: string): string {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex);
  if (!m) return "#fff";
  const n = parseInt(m[1], 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  const luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  return luminance > 0.4 ? "#111" : "#fff";
}

/** Suggested colors for the tag color picker. */
export const TAG_COLOR_PRESETS = [
  "#EF4444",
  "#F97316",
  "#EAB308",
  "#22C55E",
  "#14B8A6",
  "#3B82F6",
  "#6366F1",
  "#A855F7",
  "#EC4899",
  "#64748B",
];

export function apiErrorMessage(err: unknown): string {
  if (err instanceof ApiError) {
    const body = err.body as { error?: { message?: string } } | null;
    return body?.error?.message ?? `Request failed (${err.status})`;
  }
  return err instanceof Error ? err.message : "Request failed";
}
