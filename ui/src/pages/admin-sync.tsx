import * as React from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Loader2, Play, RefreshCw, ShieldCheck } from "lucide-react";

import type { components } from "@/gen/openapi";
import { ApiError, apiGet, apiJson } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { timeAgo } from "@/lib/format";
import { RUNNING_POLL_MS, useSyncStatus } from "@/lib/sync";
import { PageBody, PageHero, MicroLabel } from "@/components/layout";

type SyncRun = components["schemas"]["SyncRunRow"];
type SyncStats = components["schemas"]["SyncStats"];
type SyncRunsPage = components["schemas"]["ListSyncRunsResponse"];
type SyncEvent = components["schemas"]["SyncEventRow"];
type SyncEventsPage = components["schemas"]["ListSyncEventsResponse"];
type WatcherStatus = components["schemas"]["WatcherStatus"];

/** Short description of what a run covered. */
function scopeLabel(run: SyncRun): string {
  const folders = run.options?.folders;
  if (!folders) return "whole library";
  if (folders.length === 1) return folders[0];
  return `${folders.length} folders`;
}


function errorMessage(err: unknown): string {
  if (err instanceof ApiError) {
    const body = err.body as { error?: { message?: string } } | null;
    return body?.error?.message ?? err.message;
  }
  return err instanceof Error ? err.message : String(err);
}

function formatDate(value?: string | null) {
  if (!value) return "—";
  return new Date(value).toLocaleString();
}

function formatDuration(start?: string | null, end?: string | null, now = Date.now()) {
  if (!start) return "—";
  const ms = (end ? new Date(end).getTime() : now) - new Date(start).getTime();
  if (ms < 1000) return `${Math.max(ms, 0)} ms`;
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)} s`;
  const m = Math.floor(s / 60);
  return `${m}m ${Math.round(s % 60)}s`;
}

const statusStyles: Record<string, string> = {
  running: "border-signal/40 bg-signal-soft text-signal",
  queued: "border-signal/40 bg-signal-soft text-signal",
  complete: "border-input text-muted-foreground",
  failed: "border-destructive/40 bg-destructive/10 text-destructive",
  cancelled: "border-input text-dim",
};

const statusDot: Record<string, string> = {
  running: "bg-signal pulse-signal",
  queued: "bg-signal",
  complete: "bg-signal",
  failed: "bg-destructive",
  cancelled: "bg-dim",
};

function StatusBadge({ status }: { status: string }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-md border px-2 py-0.5 font-mono text-[11px] uppercase tracking-wide",
        statusStyles[status] ?? statusStyles.cancelled,
      )}
    >
      {status === "running" ? <Loader2 className="size-3 animate-spin" /> : null}
      {status}
    </span>
  );
}

/** Re-render periodically so live durations tick while a run is in progress. */
function useNow(active: boolean) {
  const [now, setNow] = React.useState(() => Date.now());
  React.useEffect(() => {
    if (!active) return;
    const t = setInterval(() => setNow(Date.now()), 500);
    return () => clearInterval(t);
  }, [active]);
  return now;
}

export function AdminSyncPage() {
  const { runId } = useParams();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const statusQ = useSyncStatus();
  const running = statusQ.data?.running ?? false;

  // When a run finishes, refresh the history and anything showing projects.
  const wasRunning = React.useRef(running);
  React.useEffect(() => {
    if (wasRunning.current && !running) {
      qc.invalidateQueries({ queryKey: ["sync"] });
      qc.invalidateQueries({ queryKey: ["projects"] });
    }
    wasRunning.current = running;
  }, [running, qc]);

  const latestRunId = statusQ.data?.run?.id;
  React.useEffect(() => {
    if (latestRunId) qc.invalidateQueries({ queryKey: ["sync", "runs"] });
  }, [latestRunId, qc]);

  const startM = useMutation({
    mutationFn: (mode: "incremental" | "full") => apiJson<SyncRun>("POST", "/sync/run", { mode }),
    onSuccess: (run) => {
      qc.invalidateQueries({ queryKey: ["sync"] });
      navigate(`/admin/sync/${run.id}`);
    },
  });

  const selectedId = runId ?? statusQ.data?.run?.id;
  const busy = running || startM.isPending;

  return (
    <div>
      <PageHero index="04" label="Sync" title="Library sync" subtitle="Reconciles the library folder with the database · files on disk are never modified">
        <div className="flex flex-wrap gap-2">
          <Button
            onClick={() => startM.mutate("incremental")}
            disabled={busy}
            title="Index new and changed files. Only hashes files whose size or modified time changed."
          >
            {busy ? <Loader2 className="animate-spin" /> : <Play />}
            {running ? "Sync running…" : "Run incremental"}
          </Button>
          <Button
            variant="outline"
            onClick={() => startM.mutate("full")}
            disabled={busy}
            title="Re-hash every file to detect content changed in place. Slower on large libraries."
          >
            <ShieldCheck />
            Full sync &amp; verify
          </Button>
        </div>
      </PageHero>

      <PageBody className="flex flex-col gap-6 pt-7 pb-16">
        {startM.isError ? (
          <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
            Could not start sync: {errorMessage(startM.error)}
          </div>
        ) : null}
        {statusQ.isError ? (
          <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
            Failed to load sync status: {errorMessage(statusQ.error)}
          </div>
        ) : null}

        <div className="flex flex-wrap items-start gap-6">
          <div className="flex min-w-0 flex-[999_1_640px] flex-col gap-5">
            {selectedId ? (
              <RunDetail runId={selectedId} isLatest={selectedId === statusQ.data?.run?.id} />
            ) : statusQ.isLoading ? (
              <div className="bg-dots h-64 animate-pulse rounded-2xl border bg-card" />
            ) : (
              <EmptyPanel>No sync has run yet. Start one to index the library folder.</EmptyPanel>
            )}
          </div>
          <aside className="flex min-w-0 flex-[1_1_340px] flex-col gap-5">
            {statusQ.data ? <WatcherCard watcher={statusQ.data.watcher} /> : null}
            <RunsList selectedId={selectedId} live={running} />
          </aside>
        </div>
      </PageBody>
    </div>
  );
}

function WatcherCard({ watcher }: { watcher: WatcherStatus }) {
  const pending = watcher.pending_folders;
  let state: string;
  let tone: string;
  let note: string | null = null;
  if (!watcher.enabled) {
    state = "OFF";
    tone = "border-input text-dim";
    note = "Disabled with LIMA_WATCHER=false. Changes are only picked up by manual syncs.";
  } else if (!watcher.active) {
    state = "STOPPED";
    tone = "border-warn/40 bg-warn/10 text-warn";
    note = `Not running: ${watcher.error ?? "unknown error"}. Use manual syncs.`;
  } else if (watcher.full_rescan_pending || pending.length > 0) {
    state = "PENDING";
    tone = "border-info/40 bg-info/10 text-info";
  } else {
    state = "ACTIVE";
    tone = "border-signal/40 bg-signal-soft text-signal";
  }

  const pendingLabel = watcher.full_rescan_pending
    ? "full rescan"
    : pending.length === 0
      ? "—"
      : pending.slice(0, 3).join(", ") + (pending.length > 3 ? ` +${pending.length - 3}` : "");

  return (
    <section aria-labelledby="watcher-h" className="flex flex-col gap-3.5 rounded-2xl border bg-card p-4.5">
      <div className="flex items-center justify-between gap-3">
        <h2 id="watcher-h" className="font-display text-[17px] font-semibold">
          Folder watcher
        </h2>
        <span className={cn("rounded-full border px-2 py-0.5 font-mono text-[11px]", tone)}>{state}</span>
      </div>
      {note ? <p className="text-sm text-muted-foreground">{note}</p> : null}
      {watcher.enabled ? (
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 font-mono text-xs">
          <dt className="text-dim">pending</dt>
          <dd className="truncate">{pendingLabel}</dd>
          <dt className="text-dim">last event</dt>
          <dd>{watcher.last_event_at ? formatDate(watcher.last_event_at) : "—"}</dd>
          <dt className="text-dim">last trigger</dt>
          <dd>{watcher.last_triggered_at ? formatDate(watcher.last_triggered_at) : "—"}</dd>
          {watcher.active && watcher.error ? (
            <>
              <dt className="text-dim">last error</dt>
              <dd className="text-warn">{watcher.error}</dd>
            </>
          ) : null}
        </dl>
      ) : null}
    </section>
  );
}

function EmptyPanel({ children }: { children: React.ReactNode }) {
  return (
    <div className="bg-dots flex items-center justify-center rounded-2xl border border-dashed p-10 text-sm text-muted-foreground">
      {children}
    </div>
  );
}

function StatsGrid({ stats }: { stats: SyncStats }) {
  const items: [string, number, string?][] = [
    ["Projects +", stats.projects_added, "text-signal"],
    ["Projects −", stats.projects_removed, "text-warn"],
    ["Assets +", stats.assets_added, "text-signal"],
    ["Assets ~", stats.assets_updated, "text-info"],
    ["Assets −", stats.assets_removed, "text-warn"],
    ["Hashed", stats.assets_hashed],
    ["Thumbs", stats.thumbnails_generated],
    ["Errors", stats.errors, "text-destructive"],
  ];
  return (
    <dl className="grid grid-cols-2 gap-px overflow-hidden rounded-xl border bg-border sm:grid-cols-4">
      {items.map(([label, value, color]) => (
        <div key={label} className="bg-card px-4 py-3.5">
          <dt>
            <MicroLabel>{label}</MicroLabel>
          </dt>
          <dd className={cn("mt-1.5 font-mono text-[22px] tabular-nums", value > 0 && color)}>{value.toLocaleString()}</dd>
        </div>
      ))}
    </dl>
  );
}

function RunsList({ selectedId, live }: { selectedId?: string; live: boolean }) {
  const runsQ = useInfiniteQuery({
    queryKey: ["sync", "runs"],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) =>
      apiGet<SyncRunsPage>(`/sync/runs?limit=20${pageParam ? `&cursor=${encodeURIComponent(pageParam)}` : ""}`),
    getNextPageParam: (last) => last.next_cursor ?? undefined,
    refetchInterval: live ? RUNNING_POLL_MS * 2 : false,
  });
  const runs = runsQ.data?.pages.flatMap((p) => p.items) ?? [];

  return (
    <section aria-labelledby="history-h" className="overflow-hidden rounded-2xl border bg-card">
      <div className="flex items-center justify-between px-4.5 pt-3 pb-2">
        <h2 id="history-h" className="font-display text-[17px] font-semibold">
          Run history
        </h2>
        <Button variant="ghost" size="icon-sm" onClick={() => runsQ.refetch()} aria-label="Refresh runs" className="text-muted-foreground">
          <RefreshCw className={cn(runsQ.isFetching && "animate-spin")} />
        </Button>
      </div>

      {runsQ.isLoading ? <div className="border-t px-4.5 py-3 text-sm text-muted-foreground">Loading…</div> : null}
      {runsQ.isError ? (
        <div className="border-t px-4.5 py-3 text-sm text-destructive">Failed to load runs: {errorMessage(runsQ.error)}</div>
      ) : null}
      {!runsQ.isLoading && runs.length === 0 ? (
        <div className="border-t px-4.5 py-3 text-sm text-muted-foreground">No runs yet.</div>
      ) : null}

      <ul>
        {runs.map((run) => {
          const s = run.stats;
          const changes = s.projects_added + s.projects_removed + s.assets_added + s.assets_updated + s.assets_removed;
          const summary =
            run.status === "failed"
              ? `failed: ${run.error || "see events"}`
              : run.status === "running"
                ? "running…"
                : [
                    run.options?.folders ? `scope: ${scopeLabel(run)}` : null,
                    changes === 0 ? "no changes" : `${changes} change${changes === 1 ? "" : "s"}`,
                    s.errors > 0 ? `${s.errors} errors` : null,
                  ]
                    .filter(Boolean)
                    .join(" · ");
          return (
            <li key={run.id}>
              <Link
                to={`/admin/sync/${run.id}`}
                aria-current={run.id === selectedId ? "true" : undefined}
                className={cn(
                  "flex items-center gap-3 border-t px-4.5 py-2.5 hover:bg-secondary/60",
                  run.id === selectedId && "bg-secondary",
                )}
              >
                <span className={cn("size-2 shrink-0 rounded-full", statusDot[run.status] ?? "bg-dim")} />
                <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span className="text-[13px] font-medium capitalize">
                    {run.mode} · {run.reason}
                  </span>
                  <span
                    className={cn(
                      "truncate font-mono text-[11px]",
                      run.status === "failed" ? "text-destructive" : "text-muted-foreground",
                    )}
                    title={summary}
                  >
                    {summary}
                  </span>
                </span>
                <span className="shrink-0 text-right font-mono text-[11px] text-dim">
                  {run.started_at ? timeAgo(run.started_at) : "—"}
                  <br />
                  {formatDuration(run.started_at, run.finished_at)}
                </span>
              </Link>
            </li>
          );
        })}
      </ul>

      {runsQ.hasNextPage ? (
        <div className="border-t p-2 text-center">
          <Button variant="ghost" size="sm" onClick={() => runsQ.fetchNextPage()} disabled={runsQ.isFetchingNextPage}>
            {runsQ.isFetchingNextPage ? "Loading…" : "Load older runs"}
          </Button>
        </div>
      ) : null}
    </section>
  );
}

function RunDetail({ runId, isLatest }: { runId: string; isLatest: boolean }) {
  const runQ = useQuery({
    queryKey: ["sync", "run", runId],
    queryFn: () => apiGet<SyncRun>(`/sync/runs/${runId}`),
    refetchInterval: (q) => (q.state.data?.status === "running" ? RUNNING_POLL_MS : false),
  });
  const run = runQ.data;
  const live = run?.status === "running";
  const now = useNow(live);

  if (runQ.isLoading) return <div className="bg-dots h-64 animate-pulse rounded-2xl border bg-card" />;
  if (runQ.isError || !run) {
    return (
      <div className="rounded-2xl border border-destructive/40 bg-destructive/10 p-4 text-sm text-destructive">
        Failed to load run: {errorMessage(runQ.error)}
      </div>
    );
  }

  const { stats } = run;
  const pct = stats.projects_total > 0 ? Math.round((stats.projects_scanned / stats.projects_total) * 100) : 0;
  const shownPct = run.status === "complete" ? 100 : pct;

  return (
    <>
      <section
        aria-label="Run"
        className="bg-grid flex flex-col gap-5 rounded-2xl border border-line-strong bg-card p-5 [background-size:32px_32px] sm:p-6"
      >
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex flex-wrap items-center gap-3">
            <span className={cn("size-2.5 rounded-full", statusDot[run.status] ?? "bg-dim")} />
            <span className="font-display text-xl font-semibold">
              {live ? "Running" : isLatest ? "Last run" : "Run"} · {run.mode}
            </span>
            <span className="rounded-md border border-input px-2 py-0.5 font-mono text-[11px] text-muted-foreground">
              {run.reason}
            </span>
            <StatusBadge status={run.status} />
          </div>
          <span className="font-mono text-xs text-muted-foreground">
            run {run.id.slice(0, 8)} · {formatDate(run.started_at)} · {formatDuration(run.started_at, run.finished_at, now)}
          </span>
        </div>

        <div className="flex flex-col gap-2">
          <div className="flex items-baseline justify-between gap-3">
            <span className="font-display text-5xl leading-none font-semibold tracking-[-0.04em] tabular-nums sm:text-[56px]">
              {shownPct}
              <span className="text-2xl text-muted-foreground">%</span>
            </span>
            <span className="font-mono text-[13px] text-muted-foreground">
              {stats.projects_scanned} / {stats.projects_total || "?"} projects
            </span>
          </div>
          <div className="h-2.5 overflow-hidden rounded-[3px] bg-secondary">
            <div
              className={cn("h-full transition-[width] duration-500", run.status === "failed" ? "bg-destructive" : "bg-scan")}
              style={{ width: `${shownPct}%` }}
            />
          </div>
          <span className="font-mono text-xs break-words text-dim">
            › scope: {run.options?.folders ? run.options.folders.join(", ") : "whole library"}
          </span>
        </div>

        {run.error ? (
          <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">{run.error}</div>
        ) : null}

        <StatsGrid stats={stats} />
      </section>

      <EventLog runId={run.id} live={live} />
    </>
  );
}

const levelStyles: Record<string, string> = {
  error: "text-[#FF7A7A]",
  warn: "text-[#F5B544]",
  info: "text-[#7DD3FC]",
  debug: "text-[#7A8597]",
  trace: "text-[#7A8597]",
};

/** Loads the whole event log, tailing new events while the run is live. */
function useRunEvents(runId: string, live: boolean) {
  const [events, setEvents] = React.useState<SyncEvent[]>([]);
  const [error, setError] = React.useState<unknown>(null);
  const [loading, setLoading] = React.useState(true);
  const lastId = React.useRef(0);

  React.useEffect(() => {
    setEvents([]);
    setError(null);
    setLoading(true);
    lastId.current = 0;
  }, [runId]);

  React.useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    async function poll() {
      try {
        // Drain all pages available right now.
        for (;;) {
          const page = await apiGet<SyncEventsPage>(`/sync/runs/${runId}/events?after=${lastId.current}&limit=500`);
          if (cancelled) return;
          if (page.items.length > 0) {
            lastId.current = page.items[page.items.length - 1].id;
            setEvents((prev) => [...prev, ...page.items]);
          }
          if (page.next_after == null) break;
        }
        setError(null);
      } catch (e) {
        if (!cancelled) setError(e);
      } finally {
        if (!cancelled) setLoading(false);
      }
      if (!cancelled && live) timer = setTimeout(poll, RUNNING_POLL_MS);
    }

    poll();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [runId, live]);

  return { events, error, loading };
}

function EventLog({ runId, live }: { runId: string; live: boolean }) {
  const { events, error, loading } = useRunEvents(runId, live);
  const [onlyProblems, setOnlyProblems] = React.useState(false);
  const shown = onlyProblems ? events.filter((e) => e.level === "warn" || e.level === "error") : events;

  // Keep the newest line in view while tailing, unless the reader scrolled up.
  const boxRef = React.useRef<HTMLDivElement>(null);
  const stick = React.useRef(true);
  React.useEffect(() => {
    const el = boxRef.current;
    if (el && live && stick.current) el.scrollTop = el.scrollHeight;
  }, [shown.length, live]);

  return (
    <section aria-labelledby="log-h" className="dark overflow-hidden rounded-2xl border border-[#1C232D] bg-terminal text-[#C9D1DC]">
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-[#1C232D] px-4 py-3">
        <h2 id="log-h" className="font-mono text-xs font-medium tracking-[0.1em] text-[#9AA4B5]">
          EVENT LOG · {events.length}
          {live ? " · LIVE" : ""}
        </h2>
        <label className="flex cursor-pointer items-center gap-2 font-mono text-[11px] text-[#9AA4B5]">
          <input
            type="checkbox"
            checked={onlyProblems}
            onChange={(e) => setOnlyProblems(e.target.checked)}
            className="accent-[#4FE3C1]"
          />
          warnings &amp; errors only
        </label>
      </div>
      {error ? <div className="px-4 pt-3 text-sm text-[#FF7A7A]">Failed to load events: {errorMessage(error)}</div> : null}
      <div
        ref={boxRef}
        onScroll={(e) => {
          const el = e.currentTarget;
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
        }}
        className="max-h-[28rem] overflow-auto px-4 py-3 font-mono text-xs leading-relaxed"
      >
        {loading ? <div className="text-[#7A8597]">loading…</div> : null}
        {!loading && shown.length === 0 ? <div className="text-[#7A8597]">no events</div> : null}
        <ol className="flex flex-col gap-1">
          {shown.map((e) => (
            <li key={e.id} className="flex gap-3.5">
              <span className="shrink-0 text-[#7A8597]">{new Date(e.created_at).toLocaleTimeString()}</span>
              <span className={cn("w-11 shrink-0 uppercase", levelStyles[e.level])}>{e.level}</span>
              <span className="min-w-0 break-words">
                <span className="text-[#E7ECF3]">{e.kind}</span> <span className="text-[#9AA4B5]">{describeEvent(e)}</span>
              </span>
            </li>
          ))}
        </ol>
      </div>
    </section>
  );
}

function describeEvent(e: SyncEvent): string {
  const d = (e.data ?? {}) as Record<string, unknown>;
  switch (e.kind) {
    case "project_added":
      return `${d.folder_path} (${d.assets} assets)`;
    case "project_updated":
      return `${d.folder_path}: +${d.assets_added} ~${d.assets_updated} -${d.assets_removed}`;
    case "project_removed":
      return `${d.folder_path} folder no longer exists`;
    case "asset_content_changed":
      return `${d.path}: content changed in place`;
    case "thumbnail_failed":
      return `${d.path ?? d.folder_path}: thumbnail failed (${d.error})`;
    case "entry_skipped":
      return `${d.path}: ${d.reason}`;
    case "project_failed":
      return `${d.folder_path}: ${d.error}`;
    case "run_finished": {
      const st = (d.stats ?? {}) as Partial<SyncStats>;
      return `${st.projects_scanned ?? 0} projects scanned · +${st.assets_added ?? 0} ~${st.assets_updated ?? 0} -${st.assets_removed ?? 0} assets · ${st.assets_hashed ?? 0} hashed · ${st.thumbnails_generated ?? 0} thumbnails`;
    }
    case "run_failed":
      return String(d.error);
    case "run_started":
      return Array.isArray(d.folders)
        ? `${d.mode ?? ""} · ${(d.folders as string[]).join(", ")}`
        : `${d.mode ?? ""} · ${d.projects_on_disk} folders on disk, ${d.projects_indexed} indexed`;
    default:
      return JSON.stringify(d);
  }
}
