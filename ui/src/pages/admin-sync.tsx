import * as React from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Loader2, Play, RefreshCw } from "lucide-react";

import type { components } from "@/gen/openapi";
import { ApiError, apiGet, apiJson } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

type SyncRun = components["schemas"]["SyncRunRow"];
type SyncStats = components["schemas"]["SyncStats"];
type SyncStatus = components["schemas"]["SyncStatusResponse"];
type SyncRunsPage = components["schemas"]["ListSyncRunsResponse"];
type SyncEvent = components["schemas"]["SyncEventRow"];
type SyncEventsPage = components["schemas"]["ListSyncEventsResponse"];

const RUNNING_POLL_MS = 1000;
const IDLE_POLL_MS = 15000;

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
  running: "bg-blue-500/15 text-blue-600 dark:text-blue-400 border-blue-500/30",
  queued: "bg-blue-500/15 text-blue-600 dark:text-blue-400 border-blue-500/30",
  complete: "bg-emerald-500/15 text-emerald-700 dark:text-emerald-400 border-emerald-500/30",
  failed: "bg-red-500/15 text-red-700 dark:text-red-400 border-red-500/30",
  cancelled: "bg-muted text-muted-foreground border-border",
};

function StatusBadge({ status }: { status: string }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-xs font-medium capitalize",
        statusStyles[status] ?? statusStyles.cancelled,
      )}
    >
      {status === "running" ? <Loader2 className="size-3 animate-spin" /> : null}
      {status}
    </span>
  );
}

function useSyncStatus() {
  return useQuery({
    queryKey: ["sync", "status"],
    queryFn: () => apiGet<SyncStatus>("/sync/status"),
    refetchInterval: (q) => (q.state.data?.running ? RUNNING_POLL_MS : IDLE_POLL_MS),
  });
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

  const startM = useMutation({
    mutationFn: () => apiJson<SyncRun>("POST", "/sync/run", { mode: "full" }),
    onSuccess: (run) => {
      qc.invalidateQueries({ queryKey: ["sync"] });
      navigate(`/admin/sync/${run.id}`);
    },
  });

  const selectedId = runId ?? statusQ.data?.run?.id;

  return (
    <div className="p-6 space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold">Library sync</h1>
          <p className="text-sm text-muted-foreground">
            Scan the library folder and reconcile projects and assets with the database. Files on disk are never
            modified.
          </p>
        </div>
        <Button onClick={() => startM.mutate()} disabled={running || startM.isPending}>
          {running || startM.isPending ? <Loader2 className="animate-spin" /> : <Play />}
          {running ? "Sync running…" : "Run full sync"}
        </Button>
      </div>

      {startM.isError ? (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
          Could not start sync: {errorMessage(startM.error)}
        </div>
      ) : null}

      <CurrentStatus status={statusQ.data} isLoading={statusQ.isLoading} error={statusQ.error} />

      <div className="grid gap-6 lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
        <RunsList selectedId={selectedId} live={running} />
        {selectedId ? <RunDetail runId={selectedId} /> : <EmptyPanel>Select a run to see its details.</EmptyPanel>}
      </div>
    </div>
  );
}

function EmptyPanel({ children }: { children: React.ReactNode }) {
  return (
    <div className="rounded-lg border border-dashed p-6 text-sm text-muted-foreground flex items-center justify-center">
      {children}
    </div>
  );
}

function CurrentStatus({
  status,
  isLoading,
  error,
}: {
  status?: SyncStatus;
  isLoading: boolean;
  error: unknown;
}) {
  const run = status?.run ?? null;
  const now = useNow(!!status?.running);

  if (isLoading) return <div className="rounded-lg border p-4 text-sm">Loading status…</div>;
  if (error) {
    return (
      <div className="rounded-lg border border-destructive/40 p-4 text-sm text-destructive">
        Failed to load sync status: {errorMessage(error)}
      </div>
    );
  }
  if (!run) return <EmptyPanel>No sync has run yet. Start one to index the library folder.</EmptyPanel>;

  const { stats } = run;
  const pct = stats.projects_total > 0 ? Math.round((stats.projects_scanned / stats.projects_total) * 100) : 0;

  return (
    <div className="rounded-lg border p-4 space-y-3">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-sm">
        <span className="font-medium">{status?.running ? "Current run" : "Last run"}</span>
        <StatusBadge status={run.status} />
        <span className="text-muted-foreground">Started {formatDate(run.started_at)}</span>
        <span className="text-muted-foreground">Duration {formatDuration(run.started_at, run.finished_at, now)}</span>
        <Link to={`/admin/sync/${run.id}`} className="text-muted-foreground underline-offset-4 hover:underline">
          Details
        </Link>
      </div>

      {run.status === "running" ? (
        <div className="space-y-1">
          <div className="h-2 w-full overflow-hidden rounded-full bg-muted">
            <div className="h-full bg-primary transition-all" style={{ width: `${pct}%` }} />
          </div>
          <div className="text-xs text-muted-foreground">
            {stats.projects_scanned} / {stats.projects_total || "?"} projects scanned
          </div>
        </div>
      ) : null}

      {run.error ? <div className="text-sm text-destructive">{run.error}</div> : null}

      <StatsGrid stats={stats} />
    </div>
  );
}

function StatsGrid({ stats }: { stats: SyncStats }) {
  const items: [string, number, string?][] = [
    ["Projects", stats.projects_total],
    ["Projects added", stats.projects_added, "text-emerald-600 dark:text-emerald-400"],
    ["Projects removed", stats.projects_removed, "text-amber-600 dark:text-amber-400"],
    ["Assets added", stats.assets_added, "text-emerald-600 dark:text-emerald-400"],
    ["Assets updated", stats.assets_updated, "text-blue-600 dark:text-blue-400"],
    ["Assets removed", stats.assets_removed, "text-amber-600 dark:text-amber-400"],
    ["Errors", stats.errors, "text-red-600 dark:text-red-400"],
  ];
  return (
    <dl className="grid grid-cols-2 sm:grid-cols-4 lg:grid-cols-7 gap-3">
      {items.map(([label, value, color]) => (
        <div key={label} className="rounded-md bg-muted/50 px-3 py-2">
          <dt className="text-xs text-muted-foreground">{label}</dt>
          <dd className={cn("text-lg font-semibold tabular-nums", value > 0 && color)}>{value}</dd>
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
    <section className="rounded-lg border">
      <div className="flex items-center justify-between border-b px-4 py-2">
        <h2 className="font-medium">History</h2>
        <Button variant="ghost" size="icon" onClick={() => runsQ.refetch()} aria-label="Refresh runs">
          <RefreshCw className={cn(runsQ.isFetching && "animate-spin")} />
        </Button>
      </div>

      {runsQ.isLoading ? <div className="p-4 text-sm">Loading…</div> : null}
      {runsQ.isError ? (
        <div className="p-4 text-sm text-destructive">Failed to load runs: {errorMessage(runsQ.error)}</div>
      ) : null}
      {!runsQ.isLoading && runs.length === 0 ? (
        <div className="p-4 text-sm text-muted-foreground">No runs yet.</div>
      ) : null}

      <ul className="divide-y">
        {runs.map((run) => {
          const s = run.stats;
          const changes = s.projects_added + s.projects_removed + s.assets_added + s.assets_updated + s.assets_removed;
          return (
            <li key={run.id}>
              <Link
                to={`/admin/sync/${run.id}`}
                className={cn(
                  "flex flex-col gap-1 px-4 py-3 text-sm hover:bg-muted/50",
                  run.id === selectedId && "bg-muted",
                )}
              >
                <div className="flex items-center justify-between gap-2">
                  <span>{formatDate(run.started_at)}</span>
                  <StatusBadge status={run.status} />
                </div>
                <div className="flex flex-wrap gap-x-3 text-xs text-muted-foreground">
                  <span>
                    {run.mode} · {run.reason}
                  </span>
                  <span>{formatDuration(run.started_at, run.finished_at)}</span>
                  <span>{changes === 0 ? "no changes" : `${changes} change${changes === 1 ? "" : "s"}`}</span>
                  {s.errors > 0 ? <span className="text-red-600 dark:text-red-400">{s.errors} errors</span> : null}
                </div>
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

function RunDetail({ runId }: { runId: string }) {
  const runQ = useQuery({
    queryKey: ["sync", "run", runId],
    queryFn: () => apiGet<SyncRun>(`/sync/runs/${runId}`),
    refetchInterval: (q) => (q.state.data?.status === "running" ? RUNNING_POLL_MS : false),
  });
  const run = runQ.data;
  const live = run?.status === "running";
  const now = useNow(live);

  if (runQ.isLoading) return <div className="rounded-lg border p-4 text-sm">Loading run…</div>;
  if (runQ.isError || !run) {
    return (
      <div className="rounded-lg border border-destructive/40 p-4 text-sm text-destructive">
        Failed to load run: {errorMessage(runQ.error)}
      </div>
    );
  }

  return (
    <section className="rounded-lg border">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-b px-4 py-2 text-sm">
        <h2 className="font-medium">Run details</h2>
        <StatusBadge status={run.status} />
        <span className="font-mono text-xs text-muted-foreground">{run.id}</span>
      </div>
      <div className="space-y-4 p-4">
        <dl className="grid grid-cols-2 sm:grid-cols-4 gap-3 text-sm">
          <Field label="Mode" value={run.mode} />
          <Field label="Reason" value={run.reason} />
          <Field label="Started" value={formatDate(run.started_at)} />
          <Field label="Duration" value={formatDuration(run.started_at, run.finished_at, now)} />
        </dl>
        {run.error ? (
          <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
            {run.error}
          </div>
        ) : null}
        <StatsGrid stats={run.stats} />
        <EventLog runId={run.id} live={live} />
      </div>
    </section>
  );
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="capitalize">{value}</dd>
    </div>
  );
}

const levelStyles: Record<string, string> = {
  error: "text-red-600 dark:text-red-400",
  warn: "text-amber-600 dark:text-amber-400",
  info: "text-foreground",
  debug: "text-muted-foreground",
  trace: "text-muted-foreground",
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

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium">Events ({events.length})</h3>
        <label className="flex items-center gap-2 text-xs text-muted-foreground">
          <input type="checkbox" checked={onlyProblems} onChange={(e) => setOnlyProblems(e.target.checked)} />
          Warnings and errors only
        </label>
      </div>
      {error ? <div className="text-sm text-destructive">Failed to load events: {errorMessage(error)}</div> : null}
      <div className="max-h-[28rem] overflow-auto rounded-md border bg-muted/30 font-mono text-xs">
        {loading ? <div className="p-3">Loading…</div> : null}
        {!loading && shown.length === 0 ? <div className="p-3 text-muted-foreground">No events.</div> : null}
        <table className="w-full">
          <tbody>
            {shown.map((e) => (
              <tr key={e.id} className="border-b last:border-0 align-top">
                <td className="whitespace-nowrap px-3 py-1.5 text-muted-foreground">
                  {new Date(e.created_at).toLocaleTimeString()}
                </td>
                <td className={cn("whitespace-nowrap px-2 py-1.5 uppercase", levelStyles[e.level])}>{e.level}</td>
                <td className="whitespace-nowrap px-2 py-1.5">{e.kind}</td>
                <td className="px-2 py-1.5 break-all text-muted-foreground">{describeEvent(e)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
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
    case "entry_skipped":
      return `${d.path}: ${d.reason}`;
    case "project_failed":
      return `${d.folder_path}: ${d.error}`;
    case "run_failed":
      return String(d.error);
    case "run_started":
      return `${d.projects_on_disk} folders on disk, ${d.projects_indexed} indexed`;
    default:
      return JSON.stringify(d);
  }
}
