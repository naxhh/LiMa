import * as React from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ExternalLink, Loader2, RefreshCw } from "lucide-react";

import { apiJson } from "@/lib/api";
import { BAMBU_STATUS_KEY, apiErrorCode, apiErrorMessage } from "@/lib/bambu";
import { COLLECTIONS_QUERY_KEY } from "@/lib/collections";
import { useCollectionJob, useCollectionSource, type CollectionSource } from "@/lib/collection-sync";
import { timeAgo } from "@/lib/format";
import { Button } from "@/components/ui/button";
import { BambuLogin } from "@/components/bambu-login";
import { CollectionJobProgress } from "@/components/collection-import";

const SOURCE_LABEL: Record<string, string> = { printables: "Printables", makerworld: "MakerWorld", thingiverse: "Thingiverse" };

/** Where an imported collection comes from, its sync state, and "Sync now". */
export function CollectionSourceBar({ collectionId }: { collectionId: string }) {
  const qc = useQueryClient();
  const sourceQ = useCollectionSource(collectionId);
  const s = sourceQ.data;
  const [jobId, setJobId] = React.useState<string | null>(null);
  const [showLogin, setShowLogin] = React.useState(false);
  const activeJob = jobId ?? s?.job_id ?? null;
  const job = useCollectionJob(activeJob).data;
  const running = job?.status === "running";

  const refresh = React.useCallback(() => {
    void qc.invalidateQueries({ queryKey: ["collection-source", collectionId] });
    void qc.invalidateQueries({ queryKey: ["collection"] });
    void qc.invalidateQueries({ queryKey: ["projects"] });
    void qc.invalidateQueries({ queryKey: COLLECTIONS_QUERY_KEY });
  }, [qc, collectionId]);

  React.useEffect(() => {
    if (job && job.status !== "running") refresh();
  }, [job, refresh]);

  const syncM = useMutation({
    mutationFn: () => apiJson<{ job_id: string }>("POST", `/collections/${encodeURIComponent(collectionId)}/source/sync`),
    onSuccess: (r) => setJobId(r.job_id),
  });
  const toggleM = useMutation({
    mutationFn: (keep: boolean) => apiJson<CollectionSource>("PATCH", `/collections/${encodeURIComponent(collectionId)}/source`, { keep_in_sync: keep }),
    onSuccess: (next) => {
      qc.setQueryData(["collection-source", collectionId], next);
      setShowLogin(false);
    },
    onError: (e) => {
      if (apiErrorCode(e) === "bambu_login_required") {
        setShowLogin(true);
        void qc.invalidateQueries({ queryKey: BAMBU_STATUS_KEY });
      }
    },
  });

  if (!s) return null;
  const label = SOURCE_LABEL[s.source] ?? s.source;
  const needsLogin = s.source === "makerworld" && s.keep_in_sync && !s.has_login;
  const loginNeededByJob = job?.error_code === "bambu_login_required";

  return (
    <div className="flex max-w-3xl flex-col gap-2.5 rounded-xl border bg-card/60 p-3 text-sm">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <a href={s.url} target="_blank" rel="noopener noreferrer" className="flex items-center gap-1.5 font-medium hover:text-signal">
          From {label}
          <ExternalLink className="size-3.5" />
        </a>
        <span className="font-mono text-xs text-muted-foreground">
          {s.keep_in_sync ? (s.interval_hours > 0 ? `kept in sync · every ${s.interval_hours} h` : "kept in sync · by hand") : "not kept in sync"}
          {" · "}
          {s.last_synced_at ? `checked ${timeAgo(s.last_synced_at)}` : "never checked"}
          {s.excluded ? ` · ${s.excluded} left out` : ""}
          {s.failed ? ` · ${s.failed} to retry` : ""}
        </span>
        <div className="ml-auto flex gap-2">
          <Button
            variant="ghost"
            size="sm"
            disabled={toggleM.isPending}
            onClick={() => toggleM.mutate(!s.keep_in_sync)}
            title={s.keep_in_sync ? "Stop importing new models and removing deleted ones" : `Follow the ${label} collection`}
          >
            {s.keep_in_sync ? "Stop syncing" : "Keep in sync"}
          </Button>
          <Button variant="outline" size="sm" disabled={running || syncM.isPending} onClick={() => syncM.mutate()}>
            {running || syncM.isPending ? <Loader2 className="animate-spin" /> : <RefreshCw />}
            Sync now
          </Button>
        </div>
      </div>
      {s.last_error && !job ? <div className="text-xs text-destructive">Last sync: {s.last_error}</div> : null}
      {needsLogin && !showLogin ? (
        <div className="text-xs text-muted-foreground">
          Background syncs need a Bambu login to download MakerWorld files.{" "}
          <button type="button" className="text-foreground underline underline-offset-4" onClick={() => setShowLogin(true)}>
            Log in
          </button>
        </div>
      ) : null}
      {syncM.isError ? <div className="text-xs text-destructive">{apiErrorMessage(syncM.error)}</div> : null}
      {toggleM.isError && !showLogin ? <div className="text-xs text-destructive">{apiErrorMessage(toggleM.error)}</div> : null}
      {job ? <CollectionJobProgress job={job} /> : null}
      {showLogin || loginNeededByJob ? (
        <BambuLogin
          onLoggedIn={() => {
            setShowLogin(false);
            // Saves the fresh login for background syncs.
            if (s.keep_in_sync) toggleM.mutate(true);
          }}
        />
      ) : null}
    </div>
  );
}
