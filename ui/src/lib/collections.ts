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

/** A collection and its projects, optionally sorted (`sort`/`order` as the API takes them). */
export function useCollection(id: string, sort?: { sort: string; order: string }) {
  const qs = sort ? `?sort=${encodeURIComponent(sort.sort)}&order=${sort.order}` : "";
  return useQuery({
    // Every sort shares the ["collection", id] prefix, so cache updates reach all of them.
    queryKey: ["collection", id, sort ? `${sort.sort}-${sort.order}` : "default"],
    queryFn: () => apiGet<CollectionDetail>(`/collections/${id}${qs}`),
    // Keep showing the list while a new sort loads (but not another collection's).
    placeholderData: (prev) => (prev?.id === id ? prev : undefined),
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
