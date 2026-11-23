import * as React from "react";
import { toast } from "sonner";
import { hasFreeWords, hasToken, quote, toggleToken } from "@/lib/search-syntax";
import { Link, useSearchParams } from "react-router-dom";
import { useInfiniteQuery } from "@tanstack/react-query";
import { X } from "lucide-react";

import { apiGet } from "@/lib/api";
import type { components } from "@/gen/openapi";
import { formatBytes } from "@/lib/format";
import { useAllTags } from "@/lib/tags";
import { useLibraryStats } from "@/lib/sync";
import { cn } from "@/lib/utils";
import { PageBody, PageHero, MicroLabel } from "@/components/layout";
import { ProjectCard, ProjectCardSkeleton } from "@/components/project-card";
import { SortSelect } from "@/components/sort-select";
import { BulkBar } from "@/components/bulk-bar";
import { Button } from "@/components/ui/button";
import { useSelection } from "@/lib/selection";
import { ListChecks } from "lucide-react";
import { LIBRARY_SORTS, PAGE_SIZE, useViewPrefs, type SortOption } from "@/lib/view-prefs";

type ListProjectsResponse = components["schemas"]["ListProjectsResponse"];

const TAG_CHIPS = 10;

function buildUrl(params: { limit: number; cursor?: string | null; query?: string; sort?: SortOption }) {
  const usp = new URLSearchParams();
  usp.set("limit", String(params.limit));
  if (params.cursor) usp.set("cursor", params.cursor);
  if (params.query && params.query.trim()) usp.set("query", params.query.trim());
  // Searches with words are ordered by relevance (the server ignores the sort then).
  if (params.sort) {
    usp.set("sort", params.sort.sort);
    usp.set("order", params.sort.order);
  }
  return `/projects?${usp.toString()}`;
}

function splitParam(v: string | null): string[] {
  return (v ?? "").split(",").map((x) => x.trim()).filter(Boolean);
}

function useDebouncedValue<T>(value: T, delayMs: number) {
  const [debounced, setDebounced] = React.useState(value);
  React.useEffect(() => {
    const t = setTimeout(() => setDebounced(value), delayMs);
    return () => clearTimeout(t);
  }, [value, delayMs]);
  return debounced;
}

export function ProjectsPage() {
  // The search box lives in the header and reads ?query=: the single source of truth for filters.
  const [searchParams, setSearchParams] = useSearchParams();
  const query = searchParams.get("query") ?? "";
  const debouncedQ = useDebouncedValue(query, 250);
  const tagsQ = useAllTags();

  // Older bookmarks kept filters as tags/formats/no_image params; fold them into `query` once.
  React.useEffect(() => {
    const tagIds = splitParam(searchParams.get("tags"));
    const formats = splitParam(searchParams.get("formats"));
    const noImage = searchParams.get("no_image") === "true";
    if (tagIds.length === 0 && formats.length === 0 && !noImage) return;
    // Tag ids map to names once the tags list is loaded (ids are gone by then).
    if (tagIds.length > 0 && !tagsQ.data) return;
    const tokens: string[] = [];
    for (const id of tagIds) {
      const tag = tagsQ.data?.find((t) => t.id === id);
      if (tag) tokens.push(`#${quote(tag.name)}`);
    }
    for (const f of formats) tokens.push(`format:${f.toLowerCase()}`);
    if (noImage) tokens.push("no:image");
    const merged = [searchParams.get("query") ?? "", ...tokens].map((x) => x.trim()).filter(Boolean).join(" ");
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.delete("tags");
        next.delete("formats");
        next.delete("no_image");
        if (merged) next.set("query", merged);
        else next.delete("query");
        return next;
      },
      { replace: true },
    );
  }, [searchParams, tagsQ.data, setSearchParams]);

  // Sort: from the URL, else the last choice, else the default.
  const view = useViewPrefs("lima.projects.view", LIBRARY_SORTS, searchParams);
  const sortOption = view.sort;
  const searching = hasFreeWords(debouncedQ);
  const hasQuery = query.trim().length > 0;

  const projectsQ = useInfiniteQuery({
    queryKey: ["projects", { limit: PAGE_SIZE, query: debouncedQ, sort: searching ? null : sortOption.id }],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) =>
      apiGet<ListProjectsResponse>(
        buildUrl({ limit: PAGE_SIZE, cursor: pageParam, query: debouncedQ, sort: searching ? undefined : sortOption }),
      ),
    getNextPageParam: (lastPage) => lastPage.next_cursor ?? undefined,
  });
  const items = projectsQ.data?.pages.flatMap((p) => p.items) ?? [];
  const selection = useSelection(items.map((p) => p.id));

  // "All" covers every match, not only the cards loaded so far.
  const selectAllMatching = async () => {
    if (!projectsQ.hasNextPage) return selection.selectAll();
    try {
      const usp = new URLSearchParams();
      if (debouncedQ.trim()) usp.set("query", debouncedQ.trim());
      const res = await apiGet<{ ids: string[]; truncated: boolean }>(`/projects/ids?${usp}`);
      selection.selectIds(res.ids);
      if (res.truncated) toast.info(`Selected the first ${res.ids.length.toLocaleString()} matches`, { description: "Bulk actions handle up to 2,000 projects at once." });
    } catch (err) {
      toast.error("Couldn't select every match", { description: err instanceof Error ? err.message : String(err) });
    }
  };

  const sentinelRef = React.useRef<HTMLDivElement | null>(null);
  const { hasNextPage, isFetchingNextPage, fetchNextPage } = projectsQ;
  React.useEffect(() => {
    const el = sentinelRef.current;
    if (!el) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting && hasNextPage && !isFetchingNextPage) fetchNextPage();
      },
      { rootMargin: "600px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [hasNextPage, isFetchingNextPage, fetchNextPage]);

  const statsQ = useLibraryStats();
  const stats = statsQ.data;

  /** Sets or removes several URL params in one navigation. */
  const setParams = (changes: Record<string, string | null>) =>
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        for (const [key, value] of Object.entries(changes)) {
          if (value) next.set(key, value);
          else next.delete(key);
        }
        return next;
      },
      { replace: true },
    );
  const setParam = (key: string, value: string | null) => setParams({ [key]: value });
  const toggleTag = (name: string) => setParam("query", toggleToken(query, `#${quote(name)}`));

  return (
    <div>
      <PageHero
        index="01"
        label="Library"
        title="Your print bay"
        subtitle={
          stats
            ? `${stats.projects.toLocaleString()} projects · ${stats.assets.toLocaleString()} files · ${formatBytes(stats.total_bytes)} on disk`
            : "\u00A0"
        }
      />

      <PageBody className="pt-5">
        <TagChips query={query} onToggle={toggleTag} />
        <div className="flex flex-wrap items-center justify-end gap-3 border-b pt-3.5 pb-4">
          <div className="flex flex-wrap items-center gap-3">
            <SortSelect options={LIBRARY_SORTS} sort={sortOption} onSort={(id) => setParams(view.chooseSort(id))} searching={hasFreeWords(query)} />
            <Button variant={selection.active ? "default" : "outline"} size="sm" className="h-8" onClick={selection.active ? selection.exit : selection.start}>
              <ListChecks />
              {selection.active ? "Done" : "Select"}
            </Button>
            <span className="font-mono text-xs text-muted-foreground">
              {projectsQ.isLoading
                ? "loading…"
                : hasQuery
                  ? `${items.length}${hasNextPage ? "+" : ""} match${items.length === 1 ? "" : "es"}`
                  : stats
                    ? `showing ${items.length} of ${stats.projects}`
                    : `showing ${items.length}`}
            </span>
            {hasQuery ? (
              <button
                type="button"
                onClick={() => setParam("query", null)}
                className="flex h-8 cursor-pointer items-center gap-1 px-1 text-[13px] text-muted-foreground hover:text-foreground"
              >
                <X className="size-3.5" /> Clear
              </button>
            ) : null}
          </div>
        </div>
      </PageBody>

      <PageBody className="pt-6 pb-16">
        {projectsQ.isError ? (
          <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-4 text-sm text-destructive">
            Failed to load projects.
          </div>
        ) : null}

        {!projectsQ.isLoading && !projectsQ.isError && items.length === 0 ? (
          <div className="bg-dots rounded-2xl border border-dashed p-10 text-center">
            <div className="font-display text-xl font-semibold">{hasQuery ? "Nothing matches" : "The bay is empty"}</div>
            <p className="mt-2 text-sm text-muted-foreground">
              {hasQuery ? (
                "Try fewer words or filters, or search by file name or tag."
              ) : (
                <>
                  Drop project folders into the library directory, or{" "}
                  <Link to="/admin/sync" className="text-signal underline-offset-4 hover:underline">
                    run a sync
                  </Link>
                  .
                </>
              )}
            </p>
          </div>
        ) : null}

        <div className="grid grid-cols-2 gap-3 sm:grid-cols-[repeat(auto-fill,minmax(260px,1fr))] sm:gap-5">
          {projectsQ.isLoading
            ? Array.from({ length: 8 }, (_, i) => <ProjectCardSkeleton key={i} />)
            : items.map((p) => (
                <ProjectCard
                  key={p.id}
                  project={p}
                  selection={selection.active ? { selected: selection.isSelected(p.id), onToggle: (range) => selection.toggle(p.id, range) } : undefined}
                />
              ))}
        </div>

        <div ref={sentinelRef} className="h-8" />
        {isFetchingNextPage || (!hasNextPage && items.length > PAGE_SIZE) ? (
          <div className="flex items-center justify-center gap-2.5 pt-4 font-mono text-xs text-dim">
            <span className="h-px w-10 bg-line-strong" />
            {isFetchingNextPage ? "loading more" : "end of library"}
            <span className="h-px w-10 bg-line-strong" />
          </div>
        ) : null}
      </PageBody>
      <BulkBar
        selection={selection}
        total={!projectsQ.hasNextPage ? items.length : !searching && stats ? stats.projects : null} onSelectAll={() => void selectAllMatching()} />
    </div>
  );
}

/** Most-used tags as filter toggles (tags already in the query always show). Several tags must all match. */
function TagChips({ query, onToggle }: { query: string; onToggle: (name: string) => void }) {
  const tagsQ = useAllTags();
  const chips = React.useMemo(() => {
    const all = tagsQ.data ?? [];
    const top = [...all]
      .filter((t) => t.project_count > 0)
      .sort((a, b) => b.project_count - a.project_count)
      .slice(0, TAG_CHIPS);
    const extra = all.filter((t) => hasToken(query, `#${quote(t.name)}`) && !top.some((x) => x.id === t.id));
    return [...extra, ...top];
  }, [tagsQ.data, query]);
  if (chips.length === 0) return null;
  return (
    <div className="flex items-center gap-2 overflow-x-auto pb-1 sm:flex-wrap sm:overflow-visible">
      <MicroLabel className="mr-1 shrink-0 text-[11px]">Tags</MicroLabel>
      {chips.map((t) => {
        const on = hasToken(query, `#${quote(t.name)}`);
        return (
          <button
            key={t.id}
            type="button"
            onClick={() => onToggle(t.name)}
            aria-pressed={on}
            className={cn(
              "flex h-8 shrink-0 cursor-pointer items-center gap-2 rounded-full border px-3 text-[13px] transition-colors",
              on ? "border-signal bg-signal-soft" : "border-input bg-card hover:border-dim",
            )}
          >
            <span className="size-2 rounded-full" style={{ backgroundColor: t.color }} />
            {t.name}
            <span className="font-mono text-[11px] text-muted-foreground">{t.project_count}</span>
          </button>
        );
      })}
      <Link to="/tags" className="shrink-0 px-2 text-[13px] text-muted-foreground hover:text-foreground">
        All tags →
      </Link>
    </div>
  );
}
