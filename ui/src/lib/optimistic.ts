/**
 * Optimistic updates: change the cached data right away, send the request in the
 * background (queued per project), and roll back with an error toast if it fails.
 */
import type { InfiniteData, QueryClient, QueryKey } from "@tanstack/react-query";
import { toast } from "sonner";

import { apiErrorMessage } from "@/lib/tags";

/** Fields every cached project shape has (list items, detail, collection pages). */
export type ProjectLike = {
  id: string;
  name: string;
  description: string;
  main_image_id?: string | null;
  tags: { id: string; name: string; color: string }[];
  collections: { id: string; name: string }[];
  is_favourite: boolean;
};

type Page = { items: ProjectLike[] };
type WithProjects = { projects: ProjectLike[] };

/** Cache keys that hold projects in some form. */
export const PROJECT_CACHE_KEYS: QueryKey[] = [["project"], ["projects"], ["collection"]];

export type Snapshot = Array<[QueryKey, unknown]>;

/** Stops in-flight refetches (so they can't overwrite the optimistic data) and copies the current data. */
export async function takeSnapshot(qc: QueryClient, keys: QueryKey[]): Promise<Snapshot> {
  await Promise.all(keys.map((queryKey) => qc.cancelQueries({ queryKey })));
  return keys.flatMap((queryKey) => qc.getQueriesData({ queryKey }));
}

export function restoreSnapshot(qc: QueryClient, snapshot: Snapshot) {
  for (const [key, data] of snapshot) qc.setQueryData(key, data);
}

function isInfinitePages(d: unknown): d is InfiniteData<Page> {
  return !!d && typeof d === "object" && Array.isArray((d as InfiniteData<Page>).pages);
}

function hasProjects(d: unknown): d is WithProjects {
  return !!d && typeof d === "object" && Array.isArray((d as WithProjects).projects);
}

/**
 * Applies `fn` to every cached copy of projects (all of them, or those matching
 * `projectId`). Returning `null` from `fn` removes the project from lists.
 */
export function patchProjects(
  qc: QueryClient,
  fn: <T extends ProjectLike>(p: T) => T | null,
  projectId?: string,
) {
  const touch = <T extends ProjectLike>(p: T): T | null => (projectId && p.id !== projectId ? p : fn(p));
  const mapList = <T extends ProjectLike>(list: T[]) => list.flatMap((p) => {
    const next = touch(p);
    return next ? [next] : [];
  });

  if (projectId) {
    qc.setQueryData<ProjectLike>(["project", projectId], (d) => (d ? (fn(d) ?? d) : d));
  } else {
    for (const [key, d] of qc.getQueriesData<ProjectLike>({ queryKey: ["project"] })) {
      if (d && typeof d === "object" && "id" in d) qc.setQueryData(key, fn(d) ?? d);
    }
  }
  qc.setQueriesData<unknown>({ queryKey: ["projects"] }, (d: unknown) =>
    isInfinitePages(d) ? { ...d, pages: d.pages.map((pg) => ({ ...pg, items: mapList(pg.items) })) } : d,
  );
  qc.setQueriesData<unknown>({ queryKey: ["collection"] }, (d: unknown) => {
    if (!hasProjects(d)) return d;
    const projects = mapList(d.projects);
    const removed = d.projects.length - projects.length;
    const count = (d as { project_count?: number }).project_count;
    return { ...d, projects, ...(removed && typeof count === "number" ? { project_count: count - removed } : {}) };
  });
}

/** Shows a failed background change. */
export function notifyFailure(title: string, err: unknown) {
  toast.error(title, { description: `${apiErrorMessage(err)}. The change was undone.` });
}

/**
 * Builds `useMutation` callbacks for an optimistic change:
 * snapshot `keys` → `apply(vars)` → request → on error restore + toast → refresh `invalidate`.
 * Refreshing waits until no other queued change for the same `mutationKey` is pending,
 * so a refetch can't briefly undo a later optimistic change.
 */
export function optimisticHandlers<TVars>(
  qc: QueryClient,
  opts: {
    mutationKey: QueryKey;
    keys?: QueryKey[];
    apply: (vars: TVars) => void;
    errorTitle: string | ((vars: TVars) => string);
    invalidate: QueryKey[];
  },
) {
  const keys = opts.keys ?? PROJECT_CACHE_KEYS;
  return {
    mutationKey: opts.mutationKey,
    // Same scope = run one at a time, in order (TanStack Query queues them).
    scope: { id: JSON.stringify(opts.mutationKey) },
    onMutate: async (vars: TVars) => {
      const snapshot = await takeSnapshot(qc, keys);
      opts.apply(vars);
      return { snapshot };
    },
    onError: (err: unknown, vars: TVars, ctx: { snapshot: Snapshot } | undefined) => {
      if (ctx) restoreSnapshot(qc, ctx.snapshot);
      notifyFailure(typeof opts.errorTitle === "function" ? opts.errorTitle(vars) : opts.errorTitle, err);
    },
    onSettled: async () => {
      // This mutation still counts as pending while onSettled runs.
      if (qc.isMutating({ mutationKey: opts.mutationKey }) > 1) return;
      await Promise.all(opts.invalidate.map((queryKey) => qc.invalidateQueries({ queryKey })));
    },
  };
}
