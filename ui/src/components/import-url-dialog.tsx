import * as React from "react";
import { Link2 } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { apiGet, apiJson } from "@/lib/api";
import type { components } from "@/gen/openapi";
import { formatBytes } from "@/lib/format";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { BambuLogin } from "@/components/bambu-login";
import { ZipExtractOptions } from "@/components/zip-extract-options";
import { useZipPrefs, zipFields } from "@/lib/zip-prefs";
import { BAMBU_STATUS_KEY, apiErrorCode, apiErrorMessage, useBambuStatus } from "@/lib/bambu";

type Preview = components["schemas"]["ImportPreviewResponse"];
type PreviewFile = components["schemas"]["ImportFile"];
type StartImportResponse = components["schemas"]["StartImportResponse"];
type ImportJob = components["schemas"]["ImportJobResponse"];

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

export function ImportUrlDialog() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [open, setOpen] = React.useState(false);

  const [url, setUrl] = React.useState("");
  const [preview, setPreview] = React.useState<Preview | null>(null);
  const [name, setName] = React.useState("");
  const [description, setDescription] = React.useState("");
  const [tagsText, setTagsText] = React.useState("");
  const [includeImages, setIncludeImages] = React.useState(true);
  const [selected, setSelected] = React.useState<Set<string>>(new Set());
  const [jobId, setJobId] = React.useState<string | null>(null);
  const [showLogin, setShowLogin] = React.useState(false);
  const [zipPrefs, setZipPrefs] = useZipPrefs();

  const reset = () => {
    setUrl("");
    setPreview(null);
    setJobId(null);
    setShowLogin(false);
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
      setSelected(defaultSelection(p));
      setShowLogin(false);
    },
  });

  const needsLogin = !!preview?.login_required && selected.size > 0;
  const bambuQ = useBambuStatus(open && needsLogin);
  const loggedIn = bambuQ.data?.logged_in ?? false;

  const selectedNames = (preview?.files ?? []).filter((f) => selected.has(f.id)).map((f) => f.name);
  const zipCount = selectedNames.filter((n) => n.toLowerCase().endsWith(".zip")).length;

  const startM = useMutation({
    mutationFn: () =>
      apiJson<StartImportResponse>("POST", "/imports", {
        url: preview!.url,
        name: name.trim(),
        description,
        tags: parseTags(tagsText),
        file_ids: [...selected],
        include_images: includeImages,
        ...zipFields(zipPrefs, selectedNames),
      }),
    onSuccess: (res) => setJobId(res.job_id),
    onError: (e) => {
      if (apiErrorCode(e) === "bambu_login_required") {
        setShowLogin(true);
        qc.invalidateQueries({ queryKey: BAMBU_STATUS_KEY });
      }
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

  React.useEffect(() => {
    if (!job) return;
    if (job.status === "complete") {
      qc.invalidateQueries({ queryKey: ["projects"] });
      if (job.warnings.length === 0) {
        setOpen(false);
        reset();
        navigate(`/projects/${job.project_id}`);
      }
    } else if (job.status === "failed" && job.error_code === "bambu_login_required") {
      setShowLogin(true);
      qc.invalidateQueries({ queryKey: BAMBU_STATUS_KEY });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [job?.status]);

  const running = !!job && (job.status === "downloading" || job.status === "importing");
  const loginBlocking = needsLogin && (showLogin || (bambuQ.isSuccess && !loggedIn));
  const canImport = !!preview && name.trim().length > 0 && !startM.isPending && !running && !loginBlocking;

  const toggle = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <Dialog
      open={open}
      onOpenChange={(v) => {
        if (running) return;
        setOpen(v);
        if (!v) reset();
      }}
    >
      <DialogTrigger asChild>
        <Button variant="outline" aria-label="Import from URL">
          <Link2 strokeWidth={2.4} />
          <span className="hidden lg:inline">Import URL</span>
        </Button>
      </DialogTrigger>

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
            placeholder="A Printables, MakerWorld or Thingiverse model link"
            autoFocus
            disabled={running}
          />
          <Button type="submit" variant={preview ? "outline" : "default"} disabled={!url.trim() || previewM.isPending || running}>
            {previewM.isPending ? "Reading…" : "Fetch"}
          </Button>
        </form>
        {previewM.isError ? <div className="text-sm text-destructive">{apiErrorMessage(previewM.error)}</div> : null}

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
                  {preview.images.map((img) => (
                    <img
                      key={img.url}
                      src={img.url}
                      alt={img.name}
                      loading="lazy"
                      referrerPolicy="no-referrer"
                      className="h-20 w-20 shrink-0 rounded-md border object-cover"
                    />
                  ))}
                </div>
                <label className="flex items-center gap-2 text-sm">
                  <input type="checkbox" checked={includeImages} onChange={(e) => setIncludeImages(e.target.checked)} />
                  Import the {preview.images.length} image{preview.images.length === 1 ? "" : "s"}
                </label>
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

            {startM.isError && apiErrorCode(startM.error) !== "existing_project" && apiErrorCode(startM.error) !== "bambu_login_required" ? (
              <div className="text-sm text-destructive">{apiErrorMessage(startM.error)}</div>
            ) : null}

            {job ? <JobProgress job={job} /> : null}
          </div>
        ) : null}

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
          ) : (
            <>
              <Button variant="outline" onClick={() => setOpen(false)} disabled={running}>
                Cancel
              </Button>
              <Button
                disabled={!canImport}
                onClick={() => {
                  setJobId(null);
                  startM.mutate();
                }}
              >
                {startM.isPending || running ? "Importing…" : job?.status === "failed" ? "Retry" : "Import"}
              </Button>
            </>
          )}
        </DialogFooter>
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
