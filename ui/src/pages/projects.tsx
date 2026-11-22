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

type Filters = { tags: string[]; formats: string[]; noImage: boolean };

/** Model formats offered as quick filters. */
const FORMAT_CHIPS = ["STL", "3MF", "STEP", "OBJ"];

function buildUrl(params: { limit: number; cursor?: string | null; query?: string; filters: Filters }) {
  const usp = new URLSearchParams();
  usp.set("limit", String(params.limit));
  if (params.cursor) usp.set("cursor", params.cursor);
  if (params.query && params.query.trim()) usp.set("query", params.query.trim());
  if (params.filters.tags.length) usp.set("tags", params.filters.tags.join(","));
  if (params.filters.formats.length) usp.set("formats", params.filters.formats.join(","));
  if (params.filters.noImage) usp.set("no_image", "true");
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
  // The search box lives in the header and writes ?query=; filters live next to it in the URL.
  const [searchParams, setSearchParams] = useSearchParams();
  const query = searchParams.get("query") ?? "";
  const debouncedQ = useDebouncedValue(query, 250);
  const tagsParam = searchParams.get("tags");
  const formatsParam = searchParams.get("formats");
  const noImage = searchParams.get("no_image") === "true";
  const filters = React.useMemo<Filters>(
    () => ({ tags: splitParam(tagsParam), formats: splitParam(formatsParam).map((f) => f.toUpperCase()), noImage }),
    [tagsParam, formatsParam, noImage],
  );
  const filtered = filters.tags.length > 0 || filters.formats.length > 0 || filters.noImage;

  const projectsQ = useInfiniteQuery({
    queryKey: ["projects", { limit: PAGE_SIZE, query: debouncedQ, filters }],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) =>
      apiGet<ListProjectsResponse>(buildUrl({ limit: PAGE_SIZE, cursor: pageParam, query: debouncedQ, filters })),
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
  const toggleIn = (key: "tags" | "formats", list: string[], value: string) => {
    const next = list.includes(value) ? list.filter((v) => v !== value) : [...list, value];
    setParam(key, next.length ? next.join(",") : null);
  };
  const clearAll = () => setParams({ query: null, tags: null, formats: null, no_image: null });

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
        <TagChips selected={filters.tags} onToggle={(id) => toggleIn("tags", filters.tags, id)} />
        <div className="flex flex-wrap items-center justify-between gap-3 border-b pt-3.5 pb-4">
          <div role="group" aria-label="File type" className="flex flex-wrap gap-1 rounded-[10px] border border-input bg-card p-[3px]">
            <FilterToggle on={filters.formats.length === 0 && !filters.noImage} onClick={() => setParams({ formats: null, no_image: null })}>
              All
            </FilterToggle>
            {FORMAT_CHIPS.map((f) => (
              <FilterToggle key={f} mono on={filters.formats.includes(f)} onClick={() => toggleIn("formats", filters.formats, f)}>
                {f}
              </FilterToggle>
            ))}
            <FilterToggle mono on={filters.noImage} onClick={() => setParam("no_image", filters.noImage ? null : "true")}>
              NO IMAGE
            </FilterToggle>
          </div>
          <span className="font-mono text-xs text-muted-foreground">
            {projectsQ.isLoading
              ? "loading…"
              : query || filtered
                ? `${items.length}${hasNextPage ? "+" : ""} match${items.length === 1 ? "" : "es"}`
                : stats
                  ? `showing ${items.length} of ${stats.projects}`
                  : `showing ${items.length}`}
          </span>
        </div>
        {query || filtered ? (
          <ActiveFilters
            query={query}
            filters={filters}
            onClearQuery={() => setParam("query", null)}
            onRemoveTag={(id) => toggleIn("tags", filters.tags, id)}
            onClearFormats={() => setParam("formats", null)}
            onClearNoImage={() => setParam("no_image", null)}
            onClearAll={clearAll}
          />
        ) : null}
      </PageBody>

      <PageBody className="pt-6 pb-16">
        {projectsQ.isError ? (
          <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-4 text-sm text-destructive">
            Failed to load projects.
          </div>
        ) : null}

        {!projectsQ.isLoading && !projectsQ.isError && items.length === 0 ? (
          <div className="bg-dots rounded-2xl border border-dashed p-10 text-center">
            <div className="font-display text-xl font-semibold">{query || filtered ? "Nothing matches" : "The bay is empty"}</div>
            <p className="mt-2 text-sm text-muted-foreground">
              {query || filtered ? (
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

/** Most-used tags as filter toggles (selected tags always shown). Several tags must all match. */
function TagChips({ selected, onToggle }: { selected: string[]; onToggle: (id: string) => void }) {
  const tagsQ = useAllTags();
  const chips = React.useMemo(() => {
    const all = tagsQ.data ?? [];
    const top = [...all]
      .filter((t) => t.project_count > 0)
      .sort((a, b) => b.project_count - a.project_count)
      .slice(0, TAG_CHIPS);
    const extra = all.filter((t) => selected.includes(t.id) && !top.some((x) => x.id === t.id));
    return [...extra, ...top];
  }, [tagsQ.data, selected]);
  if (chips.length === 0) return null;
  return (
    <div className="flex items-center gap-2 overflow-x-auto pb-1 sm:flex-wrap sm:overflow-visible">
      <MicroLabel className="mr-1 shrink-0 text-[11px]">Tags</MicroLabel>
      {chips.map((t) => {
        const on = selected.includes(t.id);
        return (
          <button
            key={t.id}
            type="button"
            onClick={() => onToggle(t.id)}
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

function FilterToggle({
  on,
  mono,
  onClick,
  children,
}: {
  on: boolean;
  mono?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={on}
      onClick={onClick}
      className={cn(
        "h-[30px] cursor-pointer rounded-[7px] px-3 transition-colors",
        mono ? "font-mono text-xs" : "text-[13px]",
        on ? "bg-secondary text-foreground" : "text-muted-foreground hover:text-foreground",
      )}
    >
      {children}
    </button>
  );
}

/** Removable pills for the search and every active filter. */
function ActiveFilters({
  query,
  filters,
  onClearQuery,
  onRemoveTag,
  onClearFormats,
  onClearNoImage,
  onClearAll,
}: {
  query: string;
  filters: Filters;
  onClearQuery: () => void;
  onRemoveTag: (id: string) => void;
  onClearFormats: () => void;
  onClearNoImage: () => void;
  onClearAll: () => void;
}) {
  const tagsQ = useAllTags();
  const tagById = new Map((tagsQ.data ?? []).map((t) => [t.id, t]));
  const count = (query ? 1 : 0) + filters.tags.length + filters.formats.length + (filters.noImage ? 1 : 0);
  return (
    <div className="flex flex-wrap items-center gap-2 pt-3 text-sm">
      <span className="text-muted-foreground">Showing</span>
      {query ? <Pill onRemove={onClearQuery} label={`Clear search ${query}`}>“{query}”</Pill> : null}
      {filters.tags.map((id) => {
        const t = tagById.get(id);
        return (
          <Pill key={id} onRemove={() => onRemoveTag(id)} label={`Remove tag ${t?.name ?? id}`}>
            <span className="size-2 rounded-full" style={{ backgroundColor: t?.color ?? "var(--dim)" }} />
            {t?.name ?? "unknown tag"}
          </Pill>
        );
      })}
      {filters.formats.length ? (
        <Pill onRemove={onClearFormats} label="Remove file type filter">
          <span className="font-mono text-xs">{filters.formats.join(" or ")}</span>
        </Pill>
      ) : null}
      {filters.noImage ? <Pill onRemove={onClearNoImage} label="Remove no image filter">without images</Pill> : null}
      {count > 1 ? (
        <button type="button" onClick={onClearAll} className="cursor-pointer px-1 text-[13px] text-muted-foreground hover:text-foreground">
          Clear all
        </button>
      ) : null}
    </div>
  );
}

function Pill({ children, onRemove, label }: { children: React.ReactNode; onRemove: () => void; label: string }) {
  return (
    <span className="inline-flex max-w-[60vw] items-center gap-1.5 rounded-full border border-signal bg-signal-soft py-0.5 pr-1 pl-2.5">
      <span className="flex min-w-0 items-center gap-1.5 truncate">{children}</span>
      <button
        type="button"
        onClick={onRemove}
        className="grid size-5 shrink-0 cursor-pointer place-items-center rounded-full hover:bg-signal/20"
        aria-label={label}
      >
        <X className="size-3" />
      </button>
    </span>
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
