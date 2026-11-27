import * as React from "react";
import { Link2 } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { apiGet, apiJson, apiNoBody, ApiError } from "@/lib/api";
import type { components } from "@/gen/openapi";
import { formatBytes } from "@/lib/format";
import { thumbUrl } from "@/lib/media";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { BambuLogin } from "@/components/bambu-login";
import { ZipExtractOptions } from "@/components/zip-extract-options";
import { CollectionImport } from "@/components/collection-import";
import { useZipPrefs, zipFields } from "@/lib/zip-prefs";
import { BAMBU_STATUS_KEY, apiErrorCode, apiErrorMessage, useBambuStatus } from "@/lib/bambu";

type Preview = components["schemas"]["ImportPreviewResponse"];
type PreviewFile = components["schemas"]["ImportFile"];
type StartImportResponse = components["schemas"]["StartImportResponse"];
type ImportJob = components["schemas"]["ImportJobResponse"];
type DuplicateImport = components["schemas"]["DuplicateImport"];

/** A project the library already has: the import stopped so it isn't copied again. */
type DuplicateInfo = { projectId: string; name: string; coverAssetId: string | null; resumeJobId: string | null };

const SOURCE_LABEL: Record<string, string> = { printables: "Printables", makerworld: "MakerWorld", thingiverse: "Thingiverse" };

function parseTags(input: string): string[] {
  const seen = new Set<string>();
  return input
    .split(/[,\n]/g)
    .map((s) => s.trim())
    .filter((t) => t && !seen.has(t.toLowerCase()) && seen.add(t.toLowerCase()));
}

/** Files ticked by default: Printables models, the first MakerWorld print profile. */
function defaultSelection(p: Preview): Set<string> {
  if (p.source === "makerworld") return new Set(p.files.slice(0, 1).map((f) => f.id));
  return new Set(p.files.filter((f) => f.kind === "model").map((f) => f.id));
}

function groupFiles(files: PreviewFile[]): [string, PreviewFile[]][] {
  const groups = new Map<string, PreviewFile[]>();
  for (const f of files) {
    const key = f.group ?? "Files";
    groups.set(key, [...(groups.get(key) ?? []), f]);
  }
  return [...groups.entries()];
}

/** The 409 the server answers with when the model is already in the library. */
function duplicateFromError(e: unknown): DuplicateInfo | null {
  if (!(e instanceof ApiError)) return null;
  const body = e.body as { error?: { code?: string; details?: Record<string, unknown> } } | null;
  if (body?.error?.code !== "duplicate_project") return null;
  const d = body.error.details ?? {};
  if (typeof d.project_id !== "string") return null;
  return {
    projectId: d.project_id,
    name: typeof d.name === "string" ? d.name : "a project",
    coverAssetId: typeof d.cover_asset_id === "string" ? d.cover_asset_id : null,
    resumeJobId: null,
  };
}

function duplicateFromJob(job: ImportJob): DuplicateInfo | null {
  const d: DuplicateImport | null | undefined = job.duplicate;
  return d ? { projectId: d.project_id, name: d.name, coverAssetId: d.cover_asset_id ?? null, resumeJobId: job.id } : null;
}

export function ImportUrlDialog({
  open,
  onOpenChange,
  showTrigger = false,
}: {
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  showTrigger?: boolean;
}) {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [uncontrolledOpen, setUncontrolledOpen] = React.useState(false);
  const isOpen = open ?? uncontrolledOpen;
  const setOpen = (v: boolean) => {
    setUncontrolledOpen(v);
    onOpenChange?.(v);
  };

  const [url, setUrl] = React.useState("");
  const [preview, setPreview] = React.useState<Preview | null>(null);
  const [name, setName] = React.useState("");
  const [description, setDescription] = React.useState("");
  const [tagsText, setTagsText] = React.useState("");
  const [includeImages, setIncludeImages] = React.useState(true);
  const [coverIndex, setCoverIndex] = React.useState(0);
  const [selected, setSelected] = React.useState<Set<string>>(new Set());
  const [jobId, setJobId] = React.useState<string | null>(null);
  const [showLogin, setShowLogin] = React.useState(false);
  const [duplicate, setDuplicate] = React.useState<DuplicateInfo | null>(null);
  const [zipPrefs, setZipPrefs] = useZipPrefs();
  const [collectionUrl, setCollectionUrl] = React.useState<string | null>(null);

  const reset = () => {
    // A duplicate job keeps its downloads for "Import anyway"; leaving throws them away.
    if (duplicate?.resumeJobId) void apiNoBody("DELETE", `/imports/${duplicate.resumeJobId}`).catch(() => {});
    setUrl("");
    setPreview(null);
    setJobId(null);
    setShowLogin(false);
    setCollectionUrl(null);
    setDuplicate(null);
    setCoverIndex(0);
    previewM.reset();
    startM.reset();
  };

  const previewM = useMutation({
    mutationFn: (u: string) => apiJson<Preview>("POST", "/imports/preview", { url: u.trim() }),
    onSuccess: (p) => {
      setPreview(p);
      setName(p.name);
      setDescription(p.description);
      setTagsText(p.tags.join(", "));
      setIncludeImages(true);
      setCoverIndex(0);
      setSelected(defaultSelection(p));
      setShowLogin(false);
      setCollectionUrl(null);
      setDuplicate(null);
    },
    onError: (e, u) => {
      // Collection links go to the collection import instead.
      if (apiErrorCode(e) === "collection_url") {
        setPreview(null);
        setCollectionUrl(u.trim());
      }
    },
  });

  const needsLogin = !!preview?.login_required && selected.size > 0;
  const bambuQ = useBambuStatus(isOpen && needsLogin);
  const loggedIn = bambuQ.data?.logged_in ?? false;

  const selectedNames = (preview?.files ?? []).filter((f) => selected.has(f.id)).map((f) => f.name);
  const zipCount = selectedNames.filter((n) => n.toLowerCase().endsWith(".zip")).length;

  const startM = useMutation({
    mutationFn: (opts: { force?: boolean; resumeJobId?: string } = {}) =>
      apiJson<StartImportResponse>("POST", "/imports", {
        url: preview!.url,
        name: name.trim(),
        description,
        tags: parseTags(tagsText),
        file_ids: [...selected],
        include_images: includeImages,
        cover_index: includeImages ? coverIndex : undefined,
        force: !!opts.force,
        resume_job_id: opts.resumeJobId,
        ...zipFields(zipPrefs, selectedNames),
      }),
    onSuccess: (res) => {
      setJobId(res.job_id);
      setDuplicate(null);
    },
    onError: (e) => {
      const code = apiErrorCode(e);
      if (code === "bambu_login_required") {
        setShowLogin(true);
        qc.invalidateQueries({ queryKey: BAMBU_STATUS_KEY });
        return;
      }
      const found = duplicateFromError(e);
      if (found) setDuplicate(found);
    },
  });

  const jobQ = useQuery({
    queryKey: ["import-job", jobId],
    queryFn: () => apiGet<ImportJob>(`/imports/${jobId}`),
    enabled: !!jobId,
    refetchInterval: (q) => {
      const s = q.state.data?.status;
      return s === "complete" || s === "failed" ? false : 1000;
    },
  });
  const job = jobQ.data;

  // Starting an import (or its progress) scrolls the dialog down so it's in view.
  const bottomRef = React.useRef<HTMLDivElement | null>(null);
  const hasJob = !!job;
  React.useEffect(() => {
    if (!isOpen || (!hasJob && !startM.isPending)) return;
    bottomRef.current?.scrollIntoView({ block: "end", behavior: "smooth" });
  }, [isOpen, hasJob, job?.status, job?.done, startM.isPending]);

  React.useEffect(() => {
    if (!job) return;
    if (job.status === "complete") {
      qc.invalidateQueries({ queryKey: ["projects"] });
      if (job.warnings.length === 0) {
        setOpen(false);
        reset();
        navigate(`/projects/${job.project_id}`);
      }
    } else if (job.status === "duplicate") {
      setDuplicate(duplicateFromJob(job));
    } else if (job.status === "failed" && job.error_code === "bambu_login_required") {
      setShowLogin(true);
      qc.invalidateQueries({ queryKey: BAMBU_STATUS_KEY });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [job?.status]);

  const running = !!job && (job.status === "downloading" || job.status === "importing");
  const loginBlocking = needsLogin && (showLogin || (bambuQ.isSuccess && !loggedIn));
  const canImport = !!preview && name.trim().length > 0 && !startM.isPending && !running && !loginBlocking && !duplicate;

  // ⌘/Ctrl+Enter starts the import from anywhere in the dialog, even while typing.
  const startRef = React.useRef<() => void>(() => {});
  React.useEffect(() => {
    startRef.current = () => {
      if (canImport) startM.mutate({});
    };
  });
  React.useEffect(() => {
    if (!isOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey || e.key !== "Enter") return;
      e.preventDefault();
      startRef.current();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [isOpen]);

  const toggle = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <Dialog
      open={isOpen}
      onOpenChange={(v) => {
        if (running) return;
        setOpen(v);
        if (!v) reset();
      }}
    >
      {showTrigger ? (
        <DialogTrigger asChild>
          <Button variant="outline" aria-label="Import from URL">
            <Link2 strokeWidth={2.4} />
            <span className="hidden lg:inline">Import URL</span>
          </Button>
        </DialogTrigger>
      ) : null}

      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Import from URL</DialogTitle>
        </DialogHeader>

        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (url.trim()) previewM.mutate(url);
          }}
        >
          <Input
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="A Printables, MakerWorld or Thingiverse model or collection link"
            autoFocus
            disabled={running}
          />
          <Button type="submit" variant={preview ? "outline" : "default"} disabled={!url.trim() || previewM.isPending || running}>
            {previewM.isPending ? "Reading…" : "Fetch"}
          </Button>
        </form>
        {previewM.isError && !collectionUrl ? (
          apiErrorCode(previewM.error) === "makerworld_captcha" ? (
            <CaptchaNotice url={url.trim()} />
          ) : (
            <div className="text-sm text-destructive">{apiErrorMessage(previewM.error)}</div>
          )
        ) : null}

        {collectionUrl ? (
          <CollectionImport
            key={collectionUrl}
            url={collectionUrl}
            onClose={() => {
              setOpen(false);
              reset();
            }}
            onDone={(id) => {
              setOpen(false);
              reset();
              navigate(`/collections/${id}`);
            }}
          />
        ) : null}

        {preview ? (
          <div className="space-y-4">
            <div className="text-xs text-muted-foreground">
              <span className="font-medium text-foreground">{SOURCE_LABEL[preview.source] ?? preview.source}</span>
              {preview.author ? ` · by ${preview.author}` : ""}
              {preview.license ? ` · ${preview.license}` : ""}
            </div>

            {preview.images.length > 0 ? (
              <div className="space-y-1.5">
                <div className="flex gap-2 overflow-x-auto pb-1">
                  {preview.images.map((img, i) => {
                    const cover = includeImages && i === coverIndex;
                    return (
                      <button
                        key={img.url}
                        type="button"
                        aria-label={`Use ${img.name} as the cover`}
                        aria-pressed={cover}
                        onClick={() => setCoverIndex(i)}
                        className={cn(
                          "relative h-20 w-20 shrink-0 overflow-hidden rounded-md border",
                          cover && "border-signal ring-2 ring-signal",
                        )}
                      >
                        <img
                          src={img.url}
                          alt={img.name}
                          loading="lazy"
                          referrerPolicy="no-referrer"
                          className="h-full w-full object-cover"
                        />
                        {cover ? (
                          <span className="absolute top-1 right-1 rounded bg-signal px-1 text-[10px] font-medium text-background">
                            Cover
                          </span>
                        ) : null}
                      </button>
                    );
                  })}
                </div>
                <label className="flex items-center gap-2 text-sm">
                  <input type="checkbox" checked={includeImages} onChange={(e) => setIncludeImages(e.target.checked)} />
                  Import the {preview.images.length} image{preview.images.length === 1 ? "" : "s"}
                </label>
                {includeImages ? <div className="text-xs text-muted-foreground">Click an image to make it the cover.</div> : null}
              </div>
            ) : null}

            <div className="space-y-1">
              <div className="text-sm font-medium">Name</div>
              <Input value={name} onChange={(e) => setName(e.target.value)} />
              {apiErrorCode(startM.error) === "existing_project" ? (
                <div className="text-xs text-destructive">A project with this name already exists. Pick another name.</div>
              ) : null}
            </div>

            <div className="space-y-1">
              <div className="text-sm font-medium">Description</div>
              <Textarea value={description} onChange={(e) => setDescription(e.target.value)} className="max-h-48" />
            </div>

            <div className="space-y-1">
              <div className="text-sm font-medium">Tags</div>
              <Input value={tagsText} onChange={(e) => setTagsText(e.target.value)} placeholder="Comma separated" />
            </div>

            <div className="space-y-2">
              <div className="text-sm font-medium">Files</div>
              {preview.files.length === 0 ? <div className="text-sm text-muted-foreground">This model has no files.</div> : null}
              {groupFiles(preview.files).map(([group, files]) => (
                <div key={group} className="space-y-1">
                  <div className="font-mono text-[10px] tracking-[0.1em] text-dim uppercase">{group}</div>
                  {files.map((f) => (
                    <label key={f.id} className="flex items-center gap-2 text-sm">
                      <input type="checkbox" checked={selected.has(f.id)} onChange={() => toggle(f.id)} />
                      <span className="min-w-0 flex-1 truncate">{f.name}</span>
                      {f.size != null ? <span className="shrink-0 text-xs text-muted-foreground">{formatBytes(f.size)}</span> : null}
                    </label>
                  ))}
                </div>
              ))}
            </div>

            <ZipExtractOptions count={zipCount} value={zipPrefs} onChange={setZipPrefs} />

            {loginBlocking ? <BambuLogin onLoggedIn={() => setShowLogin(false)} /> : null}

            {duplicate ? <DuplicateNotice duplicate={duplicate} /> : null}

            {job?.error_code === "makerworld_captcha" ? <CaptchaNotice url={preview.url} /> : null}

            {startM.isError && apiErrorCode(startM.error) !== "existing_project" && apiErrorCode(startM.error) !== "bambu_login_required" && apiErrorCode(startM.error) !== "duplicate_project" ? (
              <div className="text-sm text-destructive">{apiErrorMessage(startM.error)}</div>
            ) : null}

            {duplicate ? null : job ? <JobProgress job={job} /> : null}
          </div>
        ) : null}

        {collectionUrl ? null : (
        <DialogFooter>
          {job?.status === "complete" ? (
            <Button
              onClick={() => {
                setOpen(false);
                reset();
                navigate(`/projects/${job.project_id}`);
              }}
            >
              Open project
            </Button>
          ) : duplicate ? (
            <>
              <Button
                variant="outline"
                onClick={() => {
                  setOpen(false);
                  reset();
                }}
              >
                Cancel
              </Button>
              <Button
                disabled={startM.isPending}
                onClick={() => {
                  setJobId(null);
                  startM.mutate(duplicate.resumeJobId ? { force: true, resumeJobId: duplicate.resumeJobId } : { force: true });
                }}
              >
                {startM.isPending ? "Importing…" : "Import anyway"}
              </Button>
            </>
          ) : (
            <>
              <Button variant="outline" onClick={() => setOpen(false)} disabled={running}>
                Cancel
              </Button>
              <Button
                disabled={!canImport}
                onClick={() => {
                  setJobId(null);
                  startM.mutate({});
                }}
              >
                {startM.isPending || running ? "Importing…" : job?.status === "failed" ? "Retry" : "Import"}
              </Button>
            </>
          )}
        </DialogFooter>
        )}
        <div ref={bottomRef} aria-hidden />
      </DialogContent>
    </Dialog>
  );
}

function JobProgress({ job }: { job: ImportJob }) {
  const pct = job.total > 0 ? Math.round((job.done / job.total) * 100) : 100;
  return (
    <div className="space-y-2 rounded-xl border p-3 text-sm">
      {job.status === "failed" ? (
        <div className="text-destructive">Import failed: {job.error}</div>
      ) : job.status === "complete" ? (
        <div>Imported. Some files could not be downloaded:</div>
      ) : (
        <>
          <div className="flex justify-between gap-2 text-muted-foreground">
            <span className="truncate">{job.status === "importing" ? "Adding files to the library…" : `Downloading ${job.current ?? "…"}`}</span>
            <span className="shrink-0 font-mono text-xs">
              {job.done}/{job.total}
            </span>
          </div>
          <div className="h-1.5 overflow-hidden rounded-full bg-secondary">
            <div className="h-full bg-primary transition-all" style={{ width: `${pct}%` }} />
          </div>
        </>
      )}
      {job.warnings.length > 0 ? (
        <ul className="list-disc pl-5 text-xs text-muted-foreground">
          {job.warnings.map((w) => (
            <li key={w}>{w}</li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

/** The library already has this model: link it instead of importing a second copy. */
function DuplicateNotice({ duplicate }: { duplicate: DuplicateInfo }) {
  return (
    <div className="rounded-xl border border-signal/50 bg-signal/5 p-3 text-sm">
      <div className="flex gap-3">
        {duplicate.coverAssetId ? (
          <img
            src={thumbUrl(duplicate.projectId, duplicate.coverAssetId)}
            alt=""
            loading="lazy"
            className="h-16 w-16 shrink-0 rounded-md border object-cover"
          />
        ) : (
          <div className="grid h-16 w-16 shrink-0 place-items-center rounded-md border bg-thumb text-xs text-dim">No image</div>
        )}
        <div className="min-w-0 space-y-1">
          <div>
            This import looks a duplicate of{" "}
            <a
              href={`/projects/${duplicate.projectId}`}
              target="_blank"
              rel="noopener noreferrer"
              className="text-signal underline-offset-4 hover:underline"
            >
              “{duplicate.name}”
            </a>
          </div>
          <div className="text-muted-foreground">Confirm you want to import a copy anyway.</div>
        </div>
      </div>
    </div>
  );
}

/** MakerWorld answered 418 and wants a captcha passed in a browser. */
function CaptchaNotice({ url }: { url: string }) {
  return (
    <div className="space-y-2 rounded-xl border border-destructive/40 bg-destructive/10 p-3 text-sm">
      <div className="font-medium text-destructive">MakerWorld wants a captcha</div>
      <div className="text-muted-foreground">
        Open the model page, pass the captcha there, then retry. LiMa downloads with your Bambu login and can’t answer it
        for you.
      </div>
      <Button asChild variant="outline" size="sm">
        <a href={url} target="_blank" rel="noopener noreferrer">
          Open the model page
        </a>
      </Button>
    </div>
  );
}
