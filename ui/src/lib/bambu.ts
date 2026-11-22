import { useQuery } from "@tanstack/react-query";

import { apiGet, ApiError } from "@/lib/api";
import type { components } from "@/gen/openapi";

type BambuStatus = components["schemas"]["BambuStatusResponse"];

export const BAMBU_STATUS_KEY = ["bambu", "status"] as const;

/** Whether this browser holds a working Bambu token (checked with Bambu). */
export function useBambuStatus(enabled: boolean) {
  return useQuery({
    queryKey: BAMBU_STATUS_KEY,
    queryFn: () => apiGet<BambuStatus>("/integrations/bambu/status"),
    enabled,
    staleTime: 60_000,
  });
}

/** Machine-readable `error.code` of an API error response. */
export function apiErrorCode(e: unknown): string | undefined {
  return e instanceof ApiError ? (e.body as { error?: { code?: string } } | null)?.error?.code : undefined;
}

export function apiErrorMessage(e: unknown): string {
  if (e instanceof ApiError) {
    const msg = (e.body as { error?: { message?: string } } | null)?.error?.message;
    return msg || `Request failed (${e.status})`;
  }
  return e instanceof Error ? e.message : "Request failed";
}
