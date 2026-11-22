import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import type { components } from "@/gen/openapi";
import { apiGet, apiJson, apiNoBody } from "@/lib/api";
import { restoreMany } from "@/lib/bulk";
import { apiErrorMessage } from "@/lib/tags";
import { TRASH_KEY, refreshAfterTrash } from "@/lib/trash";

export type Duplicates = components["schemas"]["DuplicatesResponse"];
export type DuplicateGroup = components["schemas"]["DuplicateProjectGroup"];
export type DuplicateProject = components["schemas"]["DuplicateProject"];
export type SharedFileGroup = components["schemas"]["SharedFileGroup"];
type ResolveResponse = components["schemas"]["ResolveDuplicatesResponse"];
export type RemoveMode = components["schemas"]["RemoveMode"];

export const DUPLICATES_KEY = ["duplicates"] as const;

export function useDuplicates() {
  return useQuery({ queryKey: DUPLICATES_KEY, queryFn: () => apiGet<Duplicates>("/duplicates") });
}

async function refresh(qc: QueryClient) {
  await Promise.all([qc.invalidateQueries({ queryKey: DUPLICATES_KEY }), qc.invalidateQueries({ queryKey: TRASH_KEY }), refreshAfterTrash(qc)]);
}

const plural = (n: number, word: string, many = `${word}s`) => `${n} ${n === 1 ? word : many}`;

function mergedSummary(m: ResolveResponse["merged"]): string {
  const parts = [
    m.tags ? plural(m.tags, "tag") : "",
    m.collections ? plural(m.collections, "collection") : "",
    m.description ? "description" : "",
    m.source ? "source" : "",
    m.cover ? "cover" : "",
  ].filter(Boolean);
  return parts.length ? `Merged ${parts.join(", ")}.` : "Nothing new to merge.";
}

type ResolveVars = { keep: DuplicateProject; removeIds: string[]; remove: RemoveMode };

/** "Keep this one": merges the others' metadata into it, then trashes or unlists them. */
export function useResolveDuplicates() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ keep, removeIds, remove }: ResolveVars) =>
      apiJson<ResolveResponse>("POST", "/duplicates/resolve", { keep_id: keep.id, remove_ids: removeIds, remove }),
    onMutate: async ({ keep }) => {
      // The group disappears right away; it comes back if the request fails.
      await qc.cancelQueries({ queryKey: DUPLICATES_KEY });
      const before = qc.getQueryData<Duplicates>(DUPLICATES_KEY);
      qc.setQueryData<Duplicates>(DUPLICATES_KEY, (d) => {
        if (!d) return d;
        const gone = d.project_groups.find((g) => g.projects.some((p) => p.id === keep.id));
        return gone
          ? { ...d, project_groups: d.project_groups.filter((g) => g !== gone), reclaimable_bytes: d.reclaimable_bytes - gone.reclaimable_bytes }
          : d;
      });
      return { before };
    },
    onError: (err, _v, ctx) => {
      if (ctx?.before) qc.setQueryData(DUPLICATES_KEY, ctx.before);
      toast.error("Couldn't clean up the duplicates", { description: apiErrorMessage(err) });
    },
    onSuccess: (res, { keep, remove }) => {
      const removed = `${remove === "trash" && res.trash_ids.length ? "Moved" : "Removed"} ${plural(res.removed, "copy", "copies")}${
        remove === "trash" && res.trash_ids.length ? " to the trash" : ""
      }.`;
      const failed = res.failed.length ? ` ${res.failed.length} couldn't be removed.` : "";
      const trashIds = res.trash_ids;
      const notify = res.failed.length ? toast.warning : toast.success;
      notify(`Kept “${keep.name}”`, {
        description: `${mergedSummary(res.merged)} ${removed}${failed}`,
        duration: trashIds.length ? 10_000 : undefined,
        action: trashIds.length ? { label: "Undo", onClick: () => void restoreMany(qc, trashIds).then(() => refresh(qc)) } : undefined,
      });
    },
    onSettled: () => void refresh(qc),
  });
}

/** "Not duplicates": hides the group until it changes. */
export function useDismissDuplicates() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (key: string) => apiNoBody("POST", "/duplicates/dismiss", { key }),
    onMutate: async (key) => {
      await qc.cancelQueries({ queryKey: DUPLICATES_KEY });
      const before = qc.getQueryData<Duplicates>(DUPLICATES_KEY);
      qc.setQueryData<Duplicates>(DUPLICATES_KEY, (d) => {
        if (!d) return d;
        const group = d.project_groups.find((g) => g.key === key);
        return {
          ...d,
          project_groups: d.project_groups.filter((g) => g.key !== key),
          shared_files: d.shared_files.filter((f) => f.key !== key),
          reclaimable_bytes: d.reclaimable_bytes - (group?.reclaimable_bytes ?? 0),
          dismissed: d.dismissed + 1,
        };
      });
      return { before };
    },
    onError: (err, _k, ctx) => {
      if (ctx?.before) qc.setQueryData(DUPLICATES_KEY, ctx.before);
      toast.error("Couldn't hide that group", { description: apiErrorMessage(err) });
    },
    onSettled: () => void qc.invalidateQueries({ queryKey: DUPLICATES_KEY }),
  });
}

export function useClearDismissed() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => apiNoBody("DELETE", "/duplicates/dismissed"),
    onError: (err) => toast.error("Couldn't show hidden groups", { description: apiErrorMessage(err) }),
    onSettled: () => void qc.invalidateQueries({ queryKey: DUPLICATES_KEY }),
  });
}
