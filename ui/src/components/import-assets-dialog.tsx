import * as React from "react";
import { useMutation } from "@tanstack/react-query";
import { apiMultipart, apiJsonNoResponse, ApiError } from "@/lib/api";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { FileDropzone } from "@/components/file-dropzone";

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

export function ImportAssetsDialog({
  projectId,
  onImported,
}: {
  projectId: string;
  onImported: () => void;
}) {
  const [open, setOpen] = React.useState(false);
  const [files, setFiles] = React.useState<File[]>([]);
  const [bundleId, setBundleId] = React.useState<string | null>(null);
  const [mainImageName, setMainImageName] = React.useState<string>("");

  const createBundleM = useMutation({
    mutationFn: async (currentFiles: File[]) => {
      const form = new FormData();
      for (const f of currentFiles) form.append("files[]", f, f.name);
      return apiMultipart<CreateBundleResponse>("/bundles", form);
    },
    onSuccess: (b) => {
      setBundleId(b.id);
    },
    onError: () => {
      setBundleId(null);
    },
  });

  const importM = useMutation({
    mutationFn: async () => {
      if (!bundleId) throw new Error("Missing bundle id");
      const payload: ImportProjectRequest = {
        bundle_id: bundleId,
        new_main_image: mainImageName.trim() ? mainImageName.trim() : undefined,
      };
      await apiJsonNoResponse("POST", `/projects/${projectId}/import`, payload);
    },
    onSuccess: () => {
      onImported();
      setOpen(false);
      setFiles([]);
      setBundleId(null);
      setMainImageName("");
      createBundleM.reset();
      importM.reset();
    },
  });

  const imageFiles = files.filter((f) => f.type.startsWith("image/"));

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="secondary">Import files</Button>
      </DialogTrigger>

      <DialogContent>
        <DialogHeader>
          <DialogTitle>Import files</DialogTitle>
        </DialogHeader>

        <div className="space-y-3">
          <FileDropzone
            files={files}
            onFiles={(next) => {
              setFiles(next);
              const firstImage = next.find((f) => f.type.startsWith("image/"));
              setMainImageName(firstImage?.name ?? "");

              if (next.length === 0) {
                setBundleId(null);
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

          {imageFiles.length > 0 ? (
            <div className="space-y-1">
              <div className="text-sm font-medium">Main image (optional)</div>
              <select
                className="w-full rounded-md border bg-background px-3 py-2 text-sm"
                value={mainImageName}
                onChange={(e) => setMainImageName(e.target.value)}
              >
                <option value="">(none)</option>
                {imageFiles.map((f) => (
                  <option key={f.name} value={f.name}>
                    {f.name}
                  </option>
                ))}
              </select>
            </div>
          ) : null}

          {createBundleM.isError ? (
            <div className="text-sm text-destructive">
              Upload failed: {getApiErrorMessage(createBundleM.error)}
            </div>
          ) : null}

          {importM.isError ? (
            <div className="text-sm text-destructive">
              Import failed: {getApiErrorMessage(importM.error)}
            </div>
          ) : null}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => setOpen(false)} disabled={importM.isPending}>
            Cancel
          </Button>

          <Button
            onClick={() => importM.mutate()}
            disabled={!bundleId || importM.isPending || createBundleM.isPending}
          >
            {importM.isPending ? "Importing…" : "Import"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
