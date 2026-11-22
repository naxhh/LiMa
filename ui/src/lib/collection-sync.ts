import { useQuery } from "@tanstack/react-query";

import type { components } from "@/gen/openapi";
import { apiGet } from "@/lib/api";

export type CollectionJob = components["schemas"]["CollectionJob"];
export type CollectionSource = components["schemas"]["CollectionSourceResponse"];

/** Polls a collection import or sync until it finishes. */
export function useCollectionJob(jobId: string | null) {
  return useQuery({
    queryKey: ["collection-job", jobId],
    queryFn: () => apiGet<CollectionJob>(`/imports/collections/${jobId}`),
    enabled: !!jobId,
    refetchInterval: (q) => (q.state.data && q.state.data.status !== "running" ? false : 1500),
  });
}

/** Where a collection was imported from, or null when it wasn't. */
export function useCollectionSource(collectionId: string) {
  return useQuery({
    queryKey: ["collection-source", collectionId],
    queryFn: async () => {
      try {
        return await apiGet<CollectionSource>(`/collections/${encodeURIComponent(collectionId)}/source`);
      } catch (e) {
        if ((e as { status?: number }).status === 404) return null;
        throw e;
      }
    },
  });
}
