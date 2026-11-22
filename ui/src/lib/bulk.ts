/** Actions on many projects at once, applied to the UI right away (see optimistic.ts). */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { apiJson } from "@/lib/api";
import { COLLECTIONS_QUERY_KEY, type Collection } from "@/lib/collections";
import { PROJECT_CACHE_KEYS, optimisticHandlers, patchProjects, type ProjectLike } from "@/lib/optimistic";
import { TAGS_QUERY_KEY, type Tag } from "@/lib/tags";
import { TRASH_KEY, refreshAfterTrash } from "@/lib/trash";

export type BulkAction =
  | { action: "add_tags"; tags: string[] }
  | { action: "remove_tags"; tags: string[] }
  | { action: "add_to_collection"; collection_id: string }
  | { action: "remove_from_collection"; collection_id: string }
  | { action: "favourite" }
  | { action: "unfavourite" }
  | { action: "delete" }
  | { action: "remove_keep_files" };

export type BulkVars = BulkAction & { ids: string[] };
type BulkResponse = { affected: number; trash_ids: string[]; failed: string[] };

const plural = (n: number) => `${n} project${n === 1 ? "" : "s"}`;

function describe(v: BulkVars): string {
  switch (v.action) {
    case "add_tags":
      return "add tags";
    case "remove_tags":
      return "remove tags";
    case "add_to_collection":
      return "add to the collection";
    case "remove_from_collection":
      return "remove from the collection";
    case "favourite":
      return "add to favourites";
    case "unfavourite":
      return "remove from favourites";
    case "delete":
      return "delete";
    case "remove_keep_files":
      return "remove from LiMa";
  }
}

/** Restores several trashed projects one after another. */
async function restoreMany(qc: ReturnType<typeof useQueryClient>, trashIds: string[]) {
  const t = toast.loading(`Restoring ${plural(trashIds.length)}…`);
  let failed = 0;
  for (const id of trashIds) {
    try {
      await apiJson("POST", `/trash/${id}/restore`);
    } catch {
      failed++;
    }
  }
  if (failed) toast.error(`${failed} couldn't be restored`, { id: t, description: "They're still in the trash." });
  else toast.success(`Restored ${plural(trashIds.length)}`, { id: t });
  await refreshAfterTrash(qc);
}

export function useBulkAction(onDone?: () => void) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ ids, ...action }: BulkVars) => apiJson<BulkResponse>("POST", "/projects/bulk", { project_ids: ids, ...action }),
    ...optimisticHandlers<BulkVars>(qc, {
      mutationKey: ["bulk"],
      keys: [...PROJECT_CACHE_KEYS, COLLECTIONS_QUERY_KEY, TAGS_QUERY_KEY],
      apply: (v) => {
        const chosen = new Set(v.ids);
        const each = (fn: (p: ProjectLike) => ProjectLike | null) =>
          patchProjects(qc, (p) => (chosen.has(p.id) ? (fn(p) as typeof p | null) : p));
        const lower = (names: string[]) => new Set(names.map((n) => n.toLowerCase()));
        const dropFromCollection = (match: (d: { id?: string; is_favourites?: boolean }) => boolean) =>
          qc.setQueriesData<{ id?: string; is_favourites?: boolean; projects: { id: string }[]; project_count: number }>(
            { queryKey: ["collection"] },
            (d) => {
              if (!d || !match(d)) return d;
              const projects = d.projects.filter((p) => !chosen.has(p.id));
              return { ...d, projects, project_count: Math.max(0, d.project_count - (d.projects.length - projects.length)) };
            },
          );

        switch (v.action) {
          case "add_tags": {
            const known = new Map((qc.getQueryData<Tag[]>(TAGS_QUERY_KEY) ?? []).map((t) => [t.name.toLowerCase(), t]));
            const adding = v.tags.map((name) => {
              const t = known.get(name.toLowerCase());
              return t ? { id: t.id, name: t.name, color: t.color } : { id: `pending:${name}`, name, color: "#64748B" };
            });
            each((p) => {
              const have = lower(p.tags.map((t) => t.name));
              return { ...p, tags: [...p.tags, ...adding.filter((t) => !have.has(t.name.toLowerCase()))] };
            });
            break;
          }
          case "remove_tags": {
            const drop = lower(v.tags);
            each((p) => ({ ...p, tags: p.tags.filter((t) => !drop.has(t.name.toLowerCase())) }));
            break;
          }
          case "add_to_collection": {
            const name = qc.getQueryData<Collection[]>(COLLECTIONS_QUERY_KEY)?.find((c) => c.id === v.collection_id)?.name ?? "collection";
            each((p) =>
              p.collections.some((c) => c.id === v.collection_id) ? p : { ...p, collections: [...p.collections, { id: v.collection_id, name }] },
            );
            break;
          }
          case "remove_from_collection":
            each((p) => ({ ...p, collections: p.collections.filter((c) => c.id !== v.collection_id) }));
            dropFromCollection((d) => d.id === v.collection_id);
            break;
          case "favourite":
            each((p) => ({ ...p, is_favourite: true }));
            break;
          case "unfavourite":
            each((p) => ({ ...p, is_favourite: false }));
            dropFromCollection((d) => !!d.is_favourites);
            break;
          case "delete":
          case "remove_keep_files":
            each(() => null);
            break;
        }
      },
      errorTitle: (v) => `Couldn't ${describe(v)}`,
      invalidate: [...PROJECT_CACHE_KEYS, COLLECTIONS_QUERY_KEY, TAGS_QUERY_KEY, TRASH_KEY, ["ignored-folders"], ["projects", "stats"]],
    }),
    onSuccess: (res, v) => {
      onDone?.();
      if (res.failed.length) toast.error(`${plural(res.failed.length)} couldn't be changed`);
      if (v.action === "delete" && res.trash_ids.length) {
        const ids = res.trash_ids;
        toast(`Moved ${plural(ids.length)} to the trash`, {
          duration: 10000,
          action: { label: "Undo", onClick: () => void restoreMany(qc, ids) },
        });
      }
    },
  });
}

/** Downloads the projects as one zip (a form post, so the browser handles the file). */
export function downloadProjects(ids: string[]) {
  const form = document.createElement("form");
  form.method = "POST";
  form.action = "/api/projects/download";
  const input = document.createElement("input");
  input.type = "hidden";
  input.name = "ids";
  input.value = ids.join(",");
  form.appendChild(input);
  document.body.appendChild(form);
  form.submit();
  form.remove();
}
