import { useQuery, type QueryClient } from "@tanstack/react-query";

import type { components } from "@/gen/openapi";
import { apiGet } from "@/lib/api";

export type Collection = components["schemas"]["Collection"];
export type CollectionDetail = components["schemas"]["CollectionDetailResponse"];
type ListCollectionsResponse = components["schemas"]["ListCollectionsResponse"];

export const COLLECTIONS_QUERY_KEY = ["collections"] as const;

export function useCollections() {
  return useQuery({
    queryKey: COLLECTIONS_QUERY_KEY,
    queryFn: async () => (await apiGet<ListCollectionsResponse>("/collections")).items,
  });
}

export function useCollection(id: string) {
  return useQuery({
    queryKey: ["collection", id],
    queryFn: () => apiGet<CollectionDetail>(`/collections/${id}`),
  });
}

/** Refresh everything that shows collection membership. */
export async function invalidateCollections(qc: QueryClient, projectId?: string) {
  await Promise.all([
    qc.invalidateQueries({ queryKey: COLLECTIONS_QUERY_KEY }),
    qc.invalidateQueries({ queryKey: ["collection"] }),
    projectId ? qc.invalidateQueries({ queryKey: ["project", projectId] }) : null,
  ]);
}
