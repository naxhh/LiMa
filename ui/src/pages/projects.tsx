import * as React from "react";
import { Link, useSearchParams } from "react-router-dom";
import { useInfiniteQuery } from "@tanstack/react-query";
import { X } from "lucide-react";

import { apiGet } from "@/lib/api";
import type { components } from "@/gen/openapi";
import { formatBytes, timeAgo } from "@/lib/format";
import { useAllTags } from "@/lib/tags";
import { useLibraryStats, useSyncStatus } from "@/lib/sync";
import { cn } from "@/lib/utils";
import { PageBody, PageHero, MicroLabel } from "@/components/layout";
import { ProjectCard, ProjectCardSkeleton } from "@/components/project-card";

type ListProjectsResponse = components["schemas"]["ListProjectsResponse"];

const PAGE_SIZE = 48;
const TAG_CHIPS = 10;

function buildUrl(params: { limit: number; cursor?: string | null; query?: string }) {
  const usp = new URLSearchParams();
  usp.set("limit", String(params.limit));
  if (params.cursor) usp.set("cursor", params.cursor);
  if (params.query && params.query.trim()) usp.set("query", params.query.trim());
  return `/projects?${usp.toString()}`;
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
  // The search box lives in the header and writes ?query=.
  const [searchParams, setSearchParams] = useSearchParams();
  const query = searchParams.get("query") ?? "";
  const debouncedQ = useDebouncedValue(query, 250);

  const projectsQ = useInfiniteQuery({
    queryKey: ["projects", { limit: PAGE_SIZE, query: debouncedQ }],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) =>
      apiGet<ListProjectsResponse>(buildUrl({ limit: PAGE_SIZE, cursor: pageParam, query: debouncedQ })),
    getNextPageParam: (lastPage) => lastPage.next_cursor ?? undefined,
  });
  const items = projectsQ.data?.pages.flatMap((p) => p.items) ?? [];

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

  const setQuery = (q: string) => setSearchParams(q ? { query: q } : {}, { replace: true });

  return (
    <div>
      <PageHero
        index="01"
        label="Library"
        title="Your print bay"
        subtitle={
          stats
            ? `${stats.projects.toLocaleString()} projects · ${stats.assets.toLocaleString()} files · ${formatBytes(stats.total_bytes)} on disk`
            : " "
        }
      >
        <SyncBadge />
      </PageHero>

      <PageBody className="pt-5">
        <TagChips query={query} onPick={setQuery} />
        <div className="flex flex-wrap items-center justify-between gap-3 border-b pt-3.5 pb-4">
          <div className="flex min-w-0 items-center gap-2 text-sm">
            {query ? (
              <>
                <span className="text-muted-foreground">Results for</span>
                <span className="inline-flex max-w-[60vw] items-center gap-1 truncate rounded-full border border-signal bg-signal-soft py-0.5 pr-1 pl-2.5">
                  <span className="truncate">{query}</span>
                  <button
                    type="button"
                    onClick={() => setQuery("")}
                    className="grid size-5 cursor-pointer place-items-center rounded-full hover:bg-signal/20"
                    aria-label="Clear search"
                  >
                    <X className="size-3" />
                  </button>
                </span>
              </>
            ) : (
              <span className="text-muted-foreground">Recently updated</span>
            )}
          </div>
          <span className="font-mono text-xs text-muted-foreground">
            {projectsQ.isLoading
              ? "loading…"
              : query
                ? `${items.length}${hasNextPage ? "+" : ""} match${items.length === 1 ? "" : "es"}`
                : stats
                  ? `showing ${items.length} of ${stats.projects}`
                  : `showing ${items.length}`}
          </span>
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
            <div className="font-display text-xl font-semibold">{query ? "Nothing matches" : "The bay is empty"}</div>
            <p className="mt-2 text-sm text-muted-foreground">
              {query ? (
                "Try fewer words, or search by file name or tag."
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
            : items.map((p) => <ProjectCard key={p.id} project={p} />)}
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
    </div>
  );
}

/** Most-used tags as quick filters. Picking one searches for it; picking again clears. */
function TagChips({ query, onPick }: { query: string; onPick: (q: string) => void }) {
  const tagsQ = useAllTags();
  const top = React.useMemo(
    () =>
      [...(tagsQ.data ?? [])]
        .filter((t) => t.project_count > 0)
        .sort((a, b) => b.project_count - a.project_count)
        .slice(0, TAG_CHIPS),
    [tagsQ.data],
  );
  if (top.length === 0) return null;
  return (
    <div className="flex items-center gap-2 overflow-x-auto pb-1 sm:flex-wrap sm:overflow-visible">
      <MicroLabel className="mr-1 shrink-0 text-[11px]">Tags</MicroLabel>
      {top.map((t) => {
        const on = query.toLowerCase() === t.name.toLowerCase();
        return (
          <button
            key={t.id}
            type="button"
            onClick={() => onPick(on ? "" : t.name)}
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

/** Watcher / last sync summary linking to the sync admin. */
function SyncBadge() {
  const statusQ = useSyncStatus();
  const s = statusQ.data;
  if (!s) return null;

  const live = s.watcher.enabled && s.watcher.active;
  const title = s.running ? "Sync running" : live ? "Watcher live" : s.watcher.enabled ? "Watcher stopped" : "Watcher off";
  const last = s.run;
  const detail = s.running
    ? `${last?.stats.projects_scanned ?? 0} / ${last?.stats.projects_total || "?"} projects`
    : last
      ? `last sync ${timeAgo(last.finished_at ?? last.started_at)} · ${last.status === "failed" ? "failed" : last.mode}`
      : "no sync yet";

  return (
    <Link
      to="/admin/sync"
      className="flex items-center gap-3 rounded-xl border border-input bg-card px-3.5 py-2.5 transition-colors hover:border-dim"
    >
      <span
        className={cn(
          "size-2 rounded-full",
          s.running || live ? "pulse-signal bg-signal" : s.watcher.enabled ? "bg-warn" : "bg-dim",
        )}
      />
      <span className="flex flex-col gap-0.5">
        <span className="text-[13px] font-medium">{title}</span>
        <span className={cn("font-mono text-[11px]", last?.status === "failed" ? "text-destructive" : "text-muted-foreground")}>
          {detail}
        </span>
      </span>
    </Link>
  );
}
