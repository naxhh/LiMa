import { useQuery } from "@tanstack/react-query";

import type { components } from "@/gen/openapi";
import { apiGet } from "@/lib/api";

export type SyncStatus = components["schemas"]["SyncStatusResponse"];
export type LibraryStats = components["schemas"]["LibraryStatsResponse"];

export const RUNNING_POLL_MS = 1000;
const IDLE_POLL_MS = 15000;

export function useSyncStatus() {
  return useQuery({
    queryKey: ["sync", "status"],
    queryFn: () => apiGet<SyncStatus>("/sync/status"),
    refetchInterval: (q) => {
      const d = q.state.data;
      if (d?.running) return RUNNING_POLL_MS;
      if (d?.watcher.pending_folders.length || d?.watcher.full_rescan_pending) return RUNNING_POLL_MS * 2;
      return IDLE_POLL_MS;
    },
  });
}

export function useLibraryStats() {
  return useQuery({
    queryKey: ["projects", "stats"],
    queryFn: () => apiGet<LibraryStats>("/stats"),
  });
}
