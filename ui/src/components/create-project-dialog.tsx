import * as React from "react";
import { Plus } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { apiJson, apiJsonNoResponse, ApiError } from "@/lib/api";
import { uploadToBundle, type UploadBundleResponse } from "@/lib/uploads";
import { formatBytes } from "@/lib/format";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { ZipExtractOptions } from "@/components/zip-extract-options";
import { useZipPrefs, zipFields } from "@/lib/zip-prefs";
import { FileDropzone } from "@/components/file-dropzone";
import { TagChipInput } from "@/components/tag-chip-input";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";

type CreateProjectRequest = {
  name: string;
  description?: string | null;
  tags?: string[] | null;
};

type CreateProjectResponse = {
  id: string;
  folder_path: string;
};

type ImportProjectRequest = {
  bundle_id: string;
  new_main_image?: string | null; // filename
  extract_zips?: boolean;
  keep_zips?: boolean;
};

function getApiErrorMessage(e: unknown): string {
  if (e instanceof ApiError) {
    const body = e.body as { error?: { message?: string }; message?: string } | null;
    const msg = body?.error?.message || body?.message;
    return msg ? `${msg}` : `Request failed (${e.status})`;
  }
  return "Request failed";
}

export function CreateProjectDialog({
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

  const [name, setName] = React.useState("");
  const [description, setDescription] = React.useState("");
  const [tags, setTags] = React.useState<string[]>([]);

  const [files, setFiles] = React.useState<File[]>([]);
  const [mainImageName, setMainImageName] = React.useState<string>("");
  const [zipPrefs, setZipPrefs] = useZipPrefs();
  const zipCount = files.filter((f) => f.name.toLowerCase().endsWith(".zip")).length;

  const [bundleId, setBundleId] = React.useState<string | null>(null);
  const [bundleError, setBundleError] = React.useState<string | null>(null);
  const [uploadProgress, setUploadProgress] = React.useState({ sent: 0, total: 0 });

  // Create bundle immediately whenever files are added/changed
  const createBundleM = useMutation({
    mutationFn: async (currentFiles: File[]) => {
      setUploadProgress({ sent: 0, total: currentFiles.reduce((sum, file) => sum + file.size, 0) });
      return uploadToBundle(currentFiles, (sent, total) => setUploadProgress({ sent, total }));
    },
    onSuccess: (bundle: UploadBundleResponse) => {
      setBundleId(bundle.id);
      setBundleError(null);
    },
    onError: (e) => {
      setBundleId(null);
      setBundleError(getApiErrorMessage(e));
    },
  });

  // Fast project creation: create project then import using last bundleId
  const createProjectM = useMutation({
    mutationFn: async () => {
      if (!bundleId) {
        throw new Error("Missing bundleId (upload bundle first).");
      }

      const project = await apiJson<CreateProjectResponse>("POST", "/projects", {
        name: name.trim(),
        description: description.trim() ? description : "",
        tags,
      } satisfies CreateProjectRequest);

      const importPayload: ImportProjectRequest = { bundle_id: bundleId, ...zipFields(zipPrefs, files.map((f) => f.name)) };
      const chosenMain = mainImageName.trim();
      if (chosenMain) importPayload.new_main_image = chosenMain;

      await apiJsonNoResponse("POST", `/projects/${project.id}/import`, importPayload);

      return { projectId: project.id };
    },
    onSuccess: async ({ projectId }) => {
      await qc.invalidateQueries({ queryKey: ["projects"] });
      await qc.invalidateQueries({ queryKey: ["project", projectId] });

      setOpen(false);

      // reset form state (bundle can remain; but you asked "create project fast", so just reset)
      setName("");
      setDescription("");
      setTags([]);
      setFiles([]);
      setMainImageName("");
      setBundleId(null);
      setBundleError(null);
      createBundleM.reset();

      navigate(`/projects/${projectId}`);
    },
  });

  const canSubmit = name.trim().length > 0 && !!bundleId && !createProjectM.isPending;
  const uploadPercent = uploadProgress.total > 0 ? Math.min(100, Math.round((uploadProgress.sent / uploadProgress.total) * 100)) : 0;

  // ⌘/Ctrl+Enter creates the project from anywhere in the dialog.
  const submitRef = React.useRef<() => void>(() => {});
  React.useEffect(() => {
    submitRef.current = () => {
      if (canSubmit && !createBundleM.isPending) createProjectM.mutate();
    };
  });
  React.useEffect(() => {
    if (!isOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey || e.key !== "Enter") return;
      e.preventDefault();
      submitRef.current();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [isOpen]);

  return (
    <Dialog
      open={isOpen}
      onOpenChange={(v) => {
        setOpen(v);
        if (!v) {
          createProjectM.reset();
          createBundleM.reset();
          setBundleError(null);
        }
      }}
    >
      {showTrigger ? (
        <DialogTrigger asChild>
          <Button aria-label="New project">
            <Plus strokeWidth={2.4} />
            <span className="hidden sm:inline">New project</span>
          </Button>
        </DialogTrigger>
      ) : null}

      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create project</DialogTitle>
        </DialogHeader>

        <div className="space-y-4">
          <div className="space-y-1">
            <div className="text-sm font-medium">Name</div>
            <Input value={name} onChange={(e) => setName(e.target.value)} autoFocus />
          </div>

          <div className="space-y-1">
            <div className="text-sm font-medium">Description</div>
            <Textarea value={description} onChange={(e) => setDescription(e.target.value)} />
          </div>

          <div className="space-y-1">
            <div className="text-sm font-medium">Tags</div>
            <TagChipInput value={tags} onChange={setTags} />
          </div>

          <div className="space-y-2">
            <div className="text-sm font-medium">Files</div>

            <FileDropzone
              files={files}
              onFiles={(next) => {
                setFiles(next);

                const firstImage = next.find((f) => f.type.startsWith("image/"));
                setMainImageName(firstImage?.name ?? "");

                // eager bundle creation
                if (next.length === 0) {
                  setBundleId(null);
                  setBundleError(null);
                  createBundleM.reset();
                  return;
                }
                createBundleM.mutate(next);
              }}
            />

            {createBundleM.isPending ? (
              <div className="space-y-1">
                <div className="h-1 rounded-full bg-secondary">
                  <div
                    className="h-full rounded-full bg-signal transition-[width]"
                    style={{ width: `${uploadPercent}%` }}
                  />
                </div>
                <div className="font-mono text-xs text-muted-foreground">
                  {uploadPercent}% · {formatBytes(uploadProgress.sent)} / {formatBytes(uploadProgress.total)}
                </div>
              </div>
            ) : (
              <div className="text-xs text-muted-foreground">
                {bundleId
                  ? `Bundle ready: ${bundleId}`
                  : files.length > 0
                    ? "Bundle not ready yet."
                    : "Add files to create a bundle."}
              </div>
            )}

            {bundleError ? (
              <div className="text-sm text-destructive">
                Upload failed: {bundleError}
              </div>
            ) : null}

            <ZipExtractOptions count={zipCount} value={zipPrefs} onChange={setZipPrefs} />

            {files.length > 0 ? (
              <div className="space-y-1">
                <div className="text-sm font-medium">Main image (optional)</div>
                <select
                  className="h-10 w-full rounded-lg border border-input bg-card px-3 text-sm"
                  value={mainImageName}
                  onChange={(e) => setMainImageName(e.target.value)}
                >
                  <option value="">(none)</option>
                  {files
                    .filter((f) => f.type.startsWith("image/"))
                    .map((f) => (
                      <option key={f.name} value={f.name}>
                        {f.name}
                      </option>
                    ))}
                </select>
              </div>
            ) : null}
          </div>

          {createProjectM.isError ? (
            <div className="text-sm text-destructive">
              {getApiErrorMessage(createProjectM.error)}
            </div>
          ) : null}
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => setOpen(false)}
            disabled={createProjectM.isPending || createBundleM.isPending}
          >
            Cancel
          </Button>

          <Button
            disabled={!canSubmit || createBundleM.isPending}
            onClick={() => createProjectM.mutate()}
          >
            {createProjectM.isPending ? "Creating…" : "Create"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
