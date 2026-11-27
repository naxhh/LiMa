import * as React from "react";
import { Link } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Box, Check, Loader2, RefreshCw } from "lucide-react";

import type { components } from "@/gen/openapi";
import { apiJson } from "@/lib/api";
import { BAMBU_STATUS_KEY, apiErrorCode, apiErrorMessage, useBambuStatus } from "@/lib/bambu";
import { COLLECTIONS_QUERY_KEY } from "@/lib/collections";
import { useCollectionJob, type CollectionJob } from "@/lib/collection-sync";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { DialogFooter } from "@/components/ui/dialog";
import { BambuLogin } from "@/components/bambu-login";

type Preview = components["schemas"]["CollectionPreviewResponse"];
type StartResponse = components["schemas"]["StartCollectionImportResponse"];

const SOURCE_LABEL: Record<string, string> = { printables: "Printables", makerworld: "MakerWorld", thingiverse: "Thingiverse" };

/** Import every chosen model of a MakerWorld or Thingiverse collection into a new LiMa collection. */
export function CollectionImport({ url, onClose, onDone }: { url: string; onClose: () => void; onDone: (collectionId: string) => void }) {
  const qc = useQueryClient();
  const previewQ = useQuery({
    queryKey: ["collection-preview", url],
    queryFn: () => apiJson<Preview>("POST", "/imports/collections/preview", { url }),
    retry: false,
    staleTime: Infinity,
  });
  const preview = previewQ.data;
  const [selected, setSelected] = React.useState<Set<string> | null>(null);
  const [name, setName] = React.useState<string | null>(null);
  const [keepInSync, setKeepInSync] = React.useState(true);
  const [jobId, setJobId] = React.useState<string | null>(null);
  const [collectionId, setCollectionId] = React.useState<string | null>(null);
  const [showLogin, setShowLogin] = React.useState(false);

  // Everything not already in the library is ticked to start with.
  const chosen = selected ?? new Set((preview?.models ?? []).map((m) => m.id));
  const toImport = (preview?.models ?? []).filter((m) => chosen.has(m.id) && !m.project_id).length;
  const needsLogin = !!preview?.login_required && toImport > 0;
  const bambuQ = useBambuStatus(needsLogin);
  const loginBlocking = needsLogin && (showLogin || (bambuQ.isSuccess && !bambuQ.data.logged_in));

  const startM = useMutation({
    mutationFn: () =>
      apiJson<StartResponse>("POST", "/imports/collections", {
        url,
        model_ids: [...chosen],
        keep_in_sync: keepInSync,
        name: (name ?? preview?.name ?? "").trim() || undefined,
      }),
    onSuccess: (r) => {
      setJobId(r.job_id);
      setCollectionId(r.collection_id);
      void qc.invalidateQueries({ queryKey: COLLECTIONS_QUERY_KEY });
    },
    onError: (e) => {
      if (apiErrorCode(e) === "bambu_login_required") {
        setShowLogin(true);
        void qc.invalidateQueries({ queryKey: BAMBU_STATUS_KEY });
      }
    },
  });
  const job = useCollectionJob(jobId).data;
  const running = startM.isPending || job?.status === "running";

  // Starting the import (or its progress) scrolls the dialog down so it's in view.
  const bottomRef = React.useRef<HTMLDivElement | null>(null);
  React.useEffect(() => {
    if (!jobId && !startM.isPending) return;
    bottomRef.current?.scrollIntoView({ block: "end", behavior: "smooth" });
  }, [jobId, job?.status, job?.done, startM.isPending]);

  // ⌘/Ctrl+Enter imports the ticked models from anywhere in the dialog.
  const startRef = React.useRef<() => void>(() => {});
  React.useEffect(() => {
    startRef.current = () => {
      if (!running && chosen.size > 0 && !loginBlocking && !jobId) startM.mutate();
    };
  });
  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey || e.key !== "Enter") return;
      e.preventDefault();
      startRef.current();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const alreadyInLibrary = (preview?.models ?? []).filter((m) => chosen.has(m.id) && m.project_id).length;

  React.useEffect(() => {
    if (job && job.status !== "running") {
      void qc.invalidateQueries({ queryKey: ["projects"] });
      void qc.invalidateQueries({ queryKey: ["collection"] });
      void qc.invalidateQueries({ queryKey: COLLECTIONS_QUERY_KEY });
    }
  }, [job, qc]);

  if (previewQ.isPending) {
    return (
      <div className="flex items-center gap-2 py-4 font-mono text-xs text-muted-foreground">
        <Loader2 className="size-3.5 animate-spin" />
        reading the collection
      </div>
    );
  }
  if (previewQ.isError || !preview) return <div className="text-sm text-destructive">{apiErrorMessage(previewQ.error)}</div>;

  if (preview.collection_id) {
    return (
      <div className="space-y-3 rounded-xl border p-4 text-sm">
        <p>
          <span className="font-medium">{preview.name}</span> was already imported.
        </p>
        <Button asChild variant="outline">
          <Link to={`/collections/${preview.collection_id}`} onClick={onClose}>
            Open the collection
          </Link>
        </Button>
      </div>
    );
  }

  const toggle = (id: string) => {
    const next = new Set(chosen);
    if (!next.delete(id)) next.add(id);
    setSelected(next);
  };
  const allChosen = preview.models.every((m) => chosen.has(m.id));

  return (
    <div className="space-y-4">
      <div className="text-xs text-muted-foreground">
        <span className="font-medium text-foreground">{SOURCE_LABEL[preview.source] ?? preview.source} collection</span>
        {preview.author ? ` · by ${preview.author}` : ""} · {preview.models.length} model{preview.models.length === 1 ? "" : "s"}
        {preview.truncated ? " (the first 1,000)" : ""}
      </div>

      <div className="space-y-1">
        <div className="text-sm font-medium">LiMa collection</div>
        <Input value={name ?? preview.name} onChange={(e) => setName(e.target.value)} disabled={!!jobId} />
      </div>

      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <div>
            <div className="text-sm font-medium">
              Models <span className="font-mono text-xs font-normal text-muted-foreground">{chosen.size} chosen</span>
            </div>
            {alreadyInLibrary > 0 ? (
              <div className="text-xs text-muted-foreground">{alreadyInLibrary} already in the library: linked, not downloaded.</div>
            ) : null}
          </div>
          <button
            type="button"
            className="text-xs text-muted-foreground hover:text-foreground disabled:opacity-50"
            disabled={!!jobId}
            onClick={() => setSelected(allChosen ? new Set() : new Set(preview.models.map((m) => m.id)))}
          >
            {allChosen ? "Select none" : "Select all"}
          </button>
        </div>
        <ul className="max-h-72 overflow-y-auto rounded-xl border">
          {preview.models.map((m) => (
            <li key={m.id} className="border-b last:border-b-0">
              <label className="flex cursor-pointer items-center gap-3 px-3 py-2 hover:bg-secondary/60">
                <input type="checkbox" className="size-4 shrink-0 accent-primary" checked={chosen.has(m.id)} disabled={!!jobId} onChange={() => toggle(m.id)} />
                <div className="grid size-10 shrink-0 place-items-center overflow-hidden rounded-md border bg-thumb">
                  {m.cover ? <img src={m.cover} alt="" loading="lazy" referrerPolicy="no-referrer" className="size-full object-cover" /> : <Box className="size-4 text-dim" />}
                </div>
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm">{m.name}</div>
                  {m.author ? <div className="truncate text-xs text-muted-foreground">by {m.author}</div> : null}
                </div>
                {m.project_id ? (
                  <span className="shrink-0 rounded-full border px-1.5 font-mono text-[10px] text-muted-foreground" title="Added to the collection without downloading again">
                    in library
                  </span>
                ) : null}
              </label>
            </li>
          ))}
        </ul>
      </div>

      <label className="flex cursor-pointer items-start gap-2.5 rounded-xl border bg-secondary/30 p-3 text-sm">
        <input type="checkbox" className="mt-0.5 size-4 shrink-0 accent-primary" checked={keepInSync} disabled={!!jobId} onChange={(e) => setKeepInSync(e.target.checked)} />
        <span>
          <span className="flex items-center gap-1.5 font-medium">
            <RefreshCw className="size-3.5" />
            Keep in sync
          </span>
          <span className="block text-xs text-muted-foreground">
            Models added to the {SOURCE_LABEL[preview.source] ?? ""} collection are imported automatically; models removed from it go to LiMa’s trash.
            {preview.source === "makerworld" ? " Your Bambu login is stored on the server for this." : ""}
          </span>
        </span>
      </label>

      {loginBlocking && !jobId ? <BambuLogin onLoggedIn={() => setShowLogin(false)} /> : null}
      {startM.isError && apiErrorCode(startM.error) !== "bambu_login_required" ? (
        apiErrorCode(startM.error) === "makerworld_captcha" ? (
          <div className="space-y-1 rounded-xl border border-destructive/40 bg-destructive/10 p-3 text-sm">
            <div className="font-medium text-destructive">MakerWorld wants a captcha</div>
            <div className="text-muted-foreground">
              Open the collection page, pass the captcha there, then start the import again.
            </div>
            <a href={url} target="_blank" rel="noopener noreferrer" className="inline-block text-signal underline-offset-4 hover:underline">
              Open the collection page
            </a>
          </div>
        ) : (
          <div className="text-sm text-destructive">{apiErrorMessage(startM.error)}</div>
        )
      ) : null}
      {job?.error_code === "makerworld_captcha" ? (
        <a href={url} target="_blank" rel="noopener noreferrer" className="inline-block text-sm text-signal underline-offset-4 hover:underline">
          Open the collection page and pass the captcha
        </a>
      ) : null}
      {job ? <CollectionJobProgress job={job} /> : null}

      <DialogFooter>
        {job && job.status !== "running" && collectionId ? (
          <Button onClick={() => onDone(collectionId)}>
            <Check />
            Open collection
          </Button>
        ) : (
          <>
            <Button variant="outline" onClick={onClose} disabled={running}>
              Cancel
            </Button>
            <Button disabled={running || chosen.size === 0 || loginBlocking || !!jobId} onClick={() => startM.mutate()}>
              {running ? "Importing…" : `Import ${chosen.size} model${chosen.size === 1 ? "" : "s"}`}
            </Button>
          </>
        )}
      </DialogFooter>
      <div ref={bottomRef} aria-hidden />
    </div>
  );
}

export function CollectionJobProgress({ job }: { job: CollectionJob }) {
  const pct = job.total > 0 ? Math.round((job.done / job.total) * 100) : 100;
  const summary = [
    job.imported ? `${job.imported} imported` : "",
    job.linked ? `${job.linked} already in the library` : "",
    job.removed ? `${job.removed} removed` : "",
    job.failed.length ? `${job.failed.length} failed` : "",
  ].filter(Boolean);
  return (
    <div className="space-y-2 rounded-xl border p-3 text-sm">
      {job.status === "running" ? (
        <>
          <div className="flex justify-between gap-2 text-muted-foreground">
            <span className="truncate">{job.current ? `Importing ${job.current}` : "Reading the collection…"}</span>
            <span className="shrink-0 font-mono text-xs">
              {job.done}/{job.total}
            </span>
          </div>
          <div className="h-1.5 overflow-hidden rounded-full bg-secondary">
            <div className="h-full bg-primary transition-all" style={{ width: `${pct}%` }} />
          </div>
        </>
      ) : job.status === "failed" ? (
        <div className="text-destructive">{job.error}</div>
      ) : (
        <div>{summary.length ? `Done: ${summary.join(", ")}.` : "Up to date."}</div>
      )}
      {job.status === "failed" && summary.length ? <div className="text-xs text-muted-foreground">Before that: {summary.join(", ")}.</div> : null}
      {job.failed.length > 0 ? (
        <ul className="list-disc pl-5 text-xs text-muted-foreground">
          {job.failed.map((f) => (
            <li key={f.name}>
              {f.name}: {f.error}
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
