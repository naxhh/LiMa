import * as React from "react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { apiJson, apiMultipart, apiJsonNoResponse, ApiError } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { FileDropzone } from "@/components/file-dropzone";
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

type CreateBundleResponse = {
  id: string;
  files: string[];
  failed_files: string[];
};

type ImportProjectRequest = {
  bundle_id: string;
  new_main_image?: string | null; // filename
};

function getApiErrorMessage(e: unknown): string {
  if (e instanceof ApiError) {
    const body = e.body as any;
    const msg = body?.error?.message || body?.message;
    return msg ? `${msg}` : `Request failed (${e.status})`;
  }
  return "Request failed";
}

function parseTags(input: string): string[] {
  const raw = input
    .split(/[,|\n]/g)
    .map((s) => s.trim())
    .filter(Boolean);

  const seen = new Set<string>();
  const out: string[] = [];
  for (const t of raw) {
    const key = t.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(t);
  }
  return out;
}

export function CreateProjectDialog() {
  const navigate = useNavigate();
  const qc = useQueryClient();

  const [open, setOpen] = React.useState(false);

  const [name, setName] = React.useState("");
  const [description, setDescription] = React.useState("");
  const [tagsText, setTagsText] = React.useState("");

  const [files, setFiles] = React.useState<File[]>([]);
  const [mainImageName, setMainImageName] = React.useState<string>("");

  const [bundleId, setBundleId] = React.useState<string | null>(null);
  const [bundleError, setBundleError] = React.useState<string | null>(null);

  // Create bundle immediately whenever files are added/changed
  const createBundleM = useMutation({
    mutationFn: async (currentFiles: File[]) => {
      const form = new FormData();
      for (const f of currentFiles) form.append("files[]", f, f.name);
      return apiMultipart<CreateBundleResponse>("/bundles", form);
    },
    onSuccess: (bundle) => {
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
        tags: (() => {
          const t = parseTags(tagsText);
          return t.length ? t : [];
        })(),
      } satisfies CreateProjectRequest);

      const importPayload: ImportProjectRequest = { bundle_id: bundleId };
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
      setTagsText("");
      setFiles([]);
      setMainImageName("");
      setBundleId(null);
      setBundleError(null);
      createBundleM.reset();

      navigate(`/projects/${projectId}`);
    },
  });

  const canSubmit = name.trim().length > 0 && !!bundleId && !createProjectM.isPending;

  return (
    <Dialog
      open={open}
      onOpenChange={(v) => {
        setOpen(v);
        if (!v) {
          createProjectM.reset();
          createBundleM.reset();
          setBundleError(null);
        }
      }}
    >
      <DialogTrigger asChild>
        <Button>Create project</Button>
      </DialogTrigger>

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
            <Textarea
              value={tagsText}
              onChange={(e) => setTagsText(e.target.value)}
              placeholder="Comma or newline separated"
            />
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

            <div className="text-xs text-muted-foreground">
              {createBundleM.isPending
                ? "Uploading bundle…"
                : bundleId
                  ? `Bundle ready: ${bundleId}`
                  : files.length > 0
                    ? "Bundle not ready yet."
                    : "Add files to create a bundle."}
            </div>

            {bundleError ? (
              <div className="text-sm text-destructive">
                Upload failed: {bundleError}
              </div>
            ) : null}

            {files.length > 0 ? (
              <div className="space-y-1">
                <div className="text-sm font-medium">Main image (optional)</div>
                <select
                  className="w-full rounded-md border bg-background px-3 py-2 text-sm"
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
