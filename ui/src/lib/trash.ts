import { useQuery, type QueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import type { components } from "@/gen/openapi";
import { apiGet, apiJson } from "@/lib/api";
import { COLLECTIONS_QUERY_KEY } from "@/lib/collections";
import { TAGS_QUERY_KEY, apiErrorMessage } from "@/lib/tags";

export type TrashList = components["schemas"]["ListTrashResponse"];
export type TrashItem = components["schemas"]["TrashItem"];

export const TRASH_KEY = ["trash"] as const;

export function useTrash() {
  return useQuery({ queryKey: TRASH_KEY, queryFn: () => apiGet<TrashList>("/trash") });
}

/** Everything a restore or purge can change. */
export async function refreshAfterTrash(qc: QueryClient) {
  await Promise.all(
    [TRASH_KEY, ["projects"], ["collection"], COLLECTIONS_QUERY_KEY, TAGS_QUERY_KEY].map((queryKey) => qc.invalidateQueries({ queryKey })),
  );
}

/** Restores a trashed project (folder, files, tags, collections, description). Returns its new id. */
export async function restoreFromTrash(qc: QueryClient, id: string, name: string): Promise<string | null> {
  const t = toast.loading(`Restoring “${name}”…`);
  try {
    const { project_id } = await apiJson<{ project_id: string }>("POST", `/trash/${id}/restore`);
    toast.success(`Restored “${name}”`, { id: t });
    await refreshAfterTrash(qc);
    return project_id;
  } catch (err) {
    toast.error(`Couldn't restore “${name}”`, { id: t, description: apiErrorMessage(err) });
    return null;
  }
}
