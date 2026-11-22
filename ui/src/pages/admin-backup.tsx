import * as React from "react";
import { Link } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, Archive, Download, FileUp, History, Loader2, RotateCcw } from "lucide-react";

import type { components } from "@/gen/openapi";
import { ApiError, apiGet, apiJson } from "@/lib/api";
import { formatBytes, timeAgo } from "@/lib/format";
import { apiErrorMessage } from "@/lib/tags";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { MicroLabel, PageBody, PageHero } from "@/components/layout";

type BackupsResponse = components["schemas"]["ListBackupsResponse"];
type BackupFile = components["schemas"]["BackupFile"];
type MetadataBackup = components["schemas"]["MetadataBackup"];
type RestoreReport = components["schemas"]["RestoreReport"];

const BACKUPS_KEY = ["metadata", "backups"] as const;

/** Where a restore reads from: a stored backup file or an uploaded JSON. */
type RestoreSource = { kind: "stored"; name: string } | { kind: "upload"; fileName: string; backup: MetadataBackup };

function runRestore(source: RestoreSource, dryRun: boolean) {
  const q = dryRun ? "?dry_run=true" : "";
  return source.kind === "stored"
    ? apiJson<RestoreReport>("POST", `/metadata/backups/${encodeURIComponent(source.name)}/restore${q}`, {})
    : apiJson<RestoreReport>("POST", `/metadata/restore${q}`, source.backup);
}

export function AdminBackupPage() {
  const qc = useQueryClient();
  const backupsQ = useQuery({ queryKey: BACKUPS_KEY, queryFn: () => apiGet<BackupsResponse>("/metadata/backups") });
  const data = backupsQ.data;

  const createM = useMutation({
    mutationFn: () => apiJson<BackupFile>("POST", "/metadata/backups", {}),
    onSuccess: () => qc.invalidateQueries({ queryKey: BACKUPS_KEY }),
  });

  const [restoring, setRestoring] = React.useState<RestoreSource | null>(null);
  const [fileError, setFileError] = React.useState<string | null>(null);
  const fileRef = React.useRef<HTMLInputElement>(null);

  const onFile = async (file: File | undefined) => {
    setFileError(null);
    if (!file) return;
    try {
      const backup = JSON.parse(await file.text()) as MetadataBackup;
      if (backup?.format !== "lima-metadata") throw new Error("This isn't a LiMa metadata backup.");
      setRestoring({ kind: "upload", fileName: file.name, backup });
    } catch (e) {
      setFileError(e instanceof SyntaxError ? "That file isn't valid JSON." : (e as Error).message);
    } finally {
      if (fileRef.current) fileRef.current.value = "";
    }
  };

  // Backups next to the database are lost together with it.
  const sameDiskWarning = data && /(^|\/)state\/backups\/?$/.test(data.directory);

  return (
    <div>
      <PageHero
        index="05"
        label="Backup"
        title="Metadata backup"
        subtitle="Names, descriptions, covers, tags, collections and favourites · your files are never touched"
      >
        <div className="flex flex-wrap gap-2">
          <Button onClick={() => createM.mutate()} disabled={createM.isPending}>
            {createM.isPending ? <Loader2 className="animate-spin" /> : <Archive />}
            Back up now
          </Button>
          <Button asChild variant="outline">
            <a href="/api/metadata/export" download>
              <Download />
              Download export
            </a>
          </Button>
        </div>
      </PageHero>

      <PageBody className="flex flex-col gap-6 pt-7 pb-16">
        {createM.isError ? (
          <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
            Backup failed: {apiErrorMessage(createM.error)}
          </div>
        ) : null}

        <div className="flex flex-wrap items-start gap-6">
          <section aria-labelledby="stored-h" className="min-w-0 flex-[999_1_560px] overflow-hidden rounded-2xl border bg-card">
            <div className="flex flex-wrap items-center justify-between gap-2 px-4.5 pt-4 pb-3">
              <h2 id="stored-h" className="font-display text-[17px] font-semibold">
                Stored backups
              </h2>
              {data ? (
                <span className="font-mono text-[11px] text-muted-foreground">
                  {data.interval_hours ? `automatic every ${data.interval_hours} h` : "automatic backups off"} · keeping {data.keep}
                </span>
              ) : null}
            </div>
            {backupsQ.isLoading ? <div className="border-t px-4.5 py-3 text-sm text-muted-foreground">Loading…</div> : null}
            {backupsQ.isError ? (
              <div className="border-t px-4.5 py-3 text-sm text-destructive">{apiErrorMessage(backupsQ.error)}</div>
            ) : null}
            {data && data.items.length === 0 ? (
              <div className="border-t px-4.5 py-6 text-sm text-muted-foreground">
                No backups yet. The first automatic one is written a minute after the server starts, or use{" "}
                <span className="text-foreground">Back up now</span>.
              </div>
            ) : null}
            <ul>
              {data?.items.map((b, i) => (
                <li key={b.name} className="flex flex-wrap items-center gap-3 border-t px-4.5 py-2.5">
                  <History className="size-4 shrink-0 text-dim" />
                  <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                    <span className="truncate text-[13px] font-medium">
                      {new Date(b.created_at).toLocaleString()}
                      {i === 0 ? <span className="ml-2 rounded bg-signal-soft px-1.5 py-px font-mono text-[10px] text-signal">LATEST</span> : null}
                    </span>
                    <span className="truncate font-mono text-[11px] text-muted-foreground">
                      {b.name} · {formatBytes(b.size_bytes)} · {timeAgo(b.created_at)}
                    </span>
                  </span>
                  <div className="flex gap-1.5">
                    <Button asChild variant="ghost" size="icon-sm" className="text-muted-foreground">
                      <a href={`/api/metadata/backups/${encodeURIComponent(b.name)}`} download aria-label={`Download ${b.name}`}>
                        <Download />
                      </a>
                    </Button>
                    <Button variant="outline" size="sm" onClick={() => setRestoring({ kind: "stored", name: b.name })}>
                      <RotateCcw />
                      Restore
                    </Button>
                  </div>
                </li>
              ))}
            </ul>
          </section>

          <aside className="flex min-w-0 flex-[1_1_320px] flex-col gap-5">
            <section aria-labelledby="upload-h" className="flex flex-col gap-3 rounded-2xl border bg-card p-4.5">
              <h2 id="upload-h" className="font-display text-[17px] font-semibold">
                Restore from a file
              </h2>
              <p className="text-sm text-muted-foreground">
                Use an export or a backup file, for example after rebuilding the database from disk. You’ll see what
                changes before anything is written.
              </p>
              <input
                ref={fileRef}
                type="file"
                accept="application/json,.json"
                className="sr-only"
                id="backup-file"
                onChange={(e) => onFile(e.target.files?.[0])}
              />
              <Button asChild variant="outline" className="self-start">
                <label htmlFor="backup-file" className="cursor-pointer">
                  <FileUp />
                  Choose backup file…
                </label>
              </Button>
              {fileError ? <div className="text-sm text-destructive">{fileError}</div> : null}
            </section>

            <section aria-labelledby="where-h" className="flex flex-col gap-2.5 rounded-2xl border bg-card p-4.5">
              <h2 id="where-h" className="font-display text-[17px] font-semibold">
                Where backups go
              </h2>
              <code className="font-mono text-xs break-all text-muted-foreground">{data?.directory ?? "…"}</code>
              {sameDiskWarning ? (
                <p className="flex gap-2 text-sm text-warn">
                  <AlertTriangle className="mt-0.5 size-4 shrink-0" />
                  <span>
                    This is next to the database, so a lost disk takes both. Set <code className="font-mono">LIMA_BACKUP_DIR</code>{" "}
                    to another disk, or download exports now and then.
                  </span>
                </p>
              ) : null}
              <p className="text-xs text-muted-foreground">
                Configure with <code className="font-mono">LIMA_BACKUP_DIR</code>,{" "}
                <code className="font-mono">LIMA_BACKUP_INTERVAL_HOURS</code> (0 turns it off) and{" "}
                <code className="font-mono">LIMA_BACKUP_KEEP</code>.
              </p>
            </section>
          </aside>
        </div>

        <p className="text-xs text-muted-foreground">
          Rebuilt the database or moved the library? Run a{" "}
          <Link to="/admin/sync" className="text-signal underline-offset-4 hover:underline">
            sync
          </Link>{" "}
          first so the projects exist, then restore.
        </p>
      </PageBody>

      {restoring ? (
        <RestoreDialog
          source={restoring}
          onClose={() => setRestoring(null)}
          onDone={() => {
            qc.invalidateQueries({ queryKey: ["projects"] });
            qc.invalidateQueries({ queryKey: ["project"] });
            qc.invalidateQueries({ queryKey: ["tags"] });
            qc.invalidateQueries({ queryKey: ["collections"] });
            qc.invalidateQueries({ queryKey: ["collection"] });
          }}
        />
      ) : null}
    </div>
  );
}

/** Runs a dry run first, shows what would change, then restores on confirm. */
function RestoreDialog({ source, onClose, onDone }: { source: RestoreSource; onClose: () => void; onDone: () => void }) {
  const previewQ = useQuery({
    queryKey: ["metadata", "restore-preview", source.kind === "stored" ? source.name : source.fileName, source.kind],
    queryFn: () => runRestore(source, true),
    gcTime: 0,
    retry: false,
  });
  const applyM = useMutation({ mutationFn: () => runRestore(source, false), onSuccess: onDone });
  const report = applyM.data ?? previewQ.data;
  const done = applyM.isSuccess;
  const label = source.kind === "stored" ? source.name : source.fileName;

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{done ? "Metadata restored" : "Restore metadata?"}</DialogTitle>
          <DialogDescription className="break-all">
            From <span className="font-mono">{label}</span>
          </DialogDescription>
        </DialogHeader>

        {previewQ.isLoading ? (
          <div className="flex items-center gap-2 py-4 text-sm text-muted-foreground">
            <Loader2 className="size-4 animate-spin" /> Checking what would change…
          </div>
        ) : null}
        {previewQ.isError ? (
          <div className="text-sm text-destructive">
            {previewQ.error instanceof ApiError ? apiErrorMessage(previewQ.error) : "Could not read the backup."}
          </div>
        ) : null}

        {report ? (
          <div className="flex flex-col gap-4">
            <dl className="grid grid-cols-2 gap-px overflow-hidden rounded-xl border bg-border sm:grid-cols-3">
              <Stat label="Projects matched" value={report.matched_by_folder + report.matched_by_files.length} />
              <Stat label="Covers" value={report.covers_set} />
              <Stat label="Not in library" value={report.unmatched.length} tone={report.unmatched.length ? "warn" : undefined} />
              <Stat label="Tags to create" value={report.tags_created} />
              <Stat label="Tags to add" value={report.tag_links_added} />
              <Stat label="Collection adds" value={report.collection_links_added + report.collections_created} />
            </dl>

            {report.matched_by_files.length > 0 ? (
              <div className="space-y-1.5">
                <MicroLabel>Renamed folders, matched by their files</MicroLabel>
                <ul className="max-h-32 space-y-1 overflow-auto font-mono text-xs">
                  {report.matched_by_files.map((m) => (
                    <li key={m.backup_folder} className="break-all">
                      {m.backup_folder} <span className="text-signal">→</span> {m.library_folder}
                    </li>
                  ))}
                </ul>
              </div>
            ) : null}

            {report.unmatched.length > 0 ? (
              <details className="text-sm">
                <summary className="cursor-pointer text-muted-foreground">
                  {report.unmatched.length} folder{report.unmatched.length === 1 ? "" : "s"} from the backup aren’t in the library
                </summary>
                <ul className="mt-1.5 max-h-32 space-y-0.5 overflow-auto font-mono text-xs text-muted-foreground">
                  {report.unmatched.map((f) => (
                    <li key={f} className="break-all">
                      {f}
                    </li>
                  ))}
                </ul>
              </details>
            ) : null}

            {!done ? (
              <p className="text-xs text-muted-foreground">
                Names, descriptions, covers and sources are replaced with the backup’s. Tags, collections and favourites are
                added to what’s there; nothing is removed.
              </p>
            ) : null}
          </div>
        ) : null}

        {applyM.isError ? <div className="text-sm text-destructive">Restore failed: {apiErrorMessage(applyM.error)}</div> : null}

        <DialogFooter>
          {done ? (
            <Button onClick={onClose}>Done</Button>
          ) : (
            <>
              <Button variant="outline" onClick={onClose}>
                Cancel
              </Button>
              <Button disabled={!previewQ.data || applyM.isPending} onClick={() => applyM.mutate()}>
                {applyM.isPending ? <Loader2 className="animate-spin" /> : <RotateCcw />}
                Restore
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function Stat({ label, value, tone }: { label: string; value: number; tone?: "warn" }) {
  return (
    <div className="bg-card px-3 py-2.5">
      <dt>
        <MicroLabel>{label}</MicroLabel>
      </dt>
      <dd className={tone === "warn" ? "mt-1 font-mono text-lg text-warn" : "mt-1 font-mono text-lg"}>{value.toLocaleString()}</dd>
    </div>
  );
}
