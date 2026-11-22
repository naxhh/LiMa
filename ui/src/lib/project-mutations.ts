import * as React from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { apiJson, apiNoBody } from "@/lib/api";
import { invalidateCollections, type Collection } from "@/lib/collections";
import { TAGS_QUERY_KEY } from "@/lib/tags";

export type ProjectTag = { id: string; name: string; color: string };
export type Membership = { id: string; name: string };

/** Saves a project's full tag list and refreshes everything that shows tags. */
export function useSaveProjectTags(projectId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (names: string[]) => apiJson<void>("PATCH", `/projects/${projectId}`, { tags: names }),
    onSuccess: async () => {
      await Promise.all([
        qc.invalidateQueries({ queryKey: ["project", projectId] }),
        qc.invalidateQueries({ queryKey: ["projects"] }),
        qc.invalidateQueries({ queryKey: ["collection"] }),
        qc.invalidateQueries({ queryKey: TAGS_QUERY_KEY }),
      ]);
    },
  });
}

/** Add/remove a project from collections, refreshing every view that shows membership. */
export function useCollectionMembership(projectId: string) {
  const qc = useQueryClient();
  const done = async () => {
    await Promise.all([invalidateCollections(qc, projectId), qc.invalidateQueries({ queryKey: ["projects"] })]);
  };
  const add = useMutation({
    mutationFn: (collectionId: string) =>
      apiNoBody("POST", `/collections/${collectionId}/projects`, { project_id: projectId }),
    onSuccess: done,
  });
  const remove = useMutation({
    mutationFn: (collectionId: string) => apiNoBody("DELETE", `/collections/${collectionId}/projects/${projectId}`),
    onSuccess: done,
  });
  const create = useMutation({
    mutationFn: async (name: string) => {
      const c = await apiJson<Collection>("POST", "/collections", { name });
      await apiNoBody("POST", `/collections/${c.id}/projects`, { project_id: projectId });
    },
    onSuccess: done,
  });
  return { add, remove, create, error: add.error ?? remove.error ?? create.error };
}

/** Toggles the project in Favourites. Shows the new state immediately and reverts if the request fails. */
export function useFavourite(projectId: string, isFavourite: boolean) {
  const qc = useQueryClient();
  const [optimistic, setOptimistic] = React.useState<boolean | null>(null);
  // Once the server data catches up, drop the local override.
  React.useEffect(() => setOptimistic(null), [isFavourite]);

  const m = useMutation({
    mutationFn: (on: boolean) => apiNoBody(on ? "PUT" : "DELETE", `/projects/${projectId}/favourite`),
    onMutate: (on) => setOptimistic(on),
    onError: () => setOptimistic(null),
    onSettled: () =>
      Promise.all([
        qc.invalidateQueries({ queryKey: ["projects"] }),
        qc.invalidateQueries({ queryKey: ["project", projectId] }),
        qc.invalidateQueries({ queryKey: ["collections"] }),
        qc.invalidateQueries({ queryKey: ["collection"] }),
      ]),
  });
  const on = optimistic ?? isFavourite;
  return { on, toggle: () => m.mutate(!on), failed: m.isError };
}

