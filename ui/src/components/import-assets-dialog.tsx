import * as React from "react";
import { Upload } from "lucide-react";
import { useMutation } from "@tanstack/react-query";
import { apiJsonNoResponse, ApiError } from "@/lib/api";
import { uploadToBundle } from "@/lib/uploads";
import { formatBytes } from "@/lib/format";

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
import { ZipExtractOptions } from "@/components/zip-extract-options";
import { useZipPrefs, zipFields } from "@/lib/zip-prefs";

type ImportProjectRequest = {
  bundle_id: string;
  new_main_image?: string | null; // filename
  extract_zips?: boolean;
  keep_zips?: boolean;
};

function getApiErrorMessage(e: unknown): string {
  if (e instanceof ApiError) {
    const body = e.body as { error?: { message?: string }; message?: string } | undefined;
    const msg = body?.error?.message || body?.message;
    return msg ? `${msg}` : `Request failed (${e.status})`;
  }
  return "Request failed";
}

export function ImportAssetsDialog({
  projectId,
  onImported,
  open: openProp,
  onOpenChange,
}: {
  projectId: string;
  onImported: () => void;
  /** Controls the dialog from outside (e.g. the `i` key). */
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}) {
  const [ownOpen, setOwnOpen] = React.useState(false);
  const open = openProp ?? ownOpen;
  const setOpen = onOpenChange ?? setOwnOpen;
  const [files, setFiles] = React.useState<File[]>([]);
  const [bundleId, setBundleId] = React.useState<string | null>(null);
  const [mainImageName, setMainImageName] = React.useState<string>("");
  const [uploadProgress, setUploadProgress] = React.useState({ sent: 0, total: 0 });
  const [zipPrefs, setZipPrefs] = useZipPrefs();
  const zipCount = files.filter((f) => f.name.toLowerCase().endsWith(".zip")).length;

  const createBundleM = useMutation({
    mutationFn: async (currentFiles: File[]) => {
      setUploadProgress({ sent: 0, total: currentFiles.reduce((sum, file) => sum + file.size, 0) });
      return uploadToBundle(currentFiles, (sent, total) => setUploadProgress({ sent, total }));
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
        ...zipFields(zipPrefs, files.map((f) => f.name)),
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
  const uploadPercent = uploadProgress.total > 0 ? Math.min(100, Math.round((uploadProgress.sent / uploadProgress.total) * 100)) : 0;

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="outline">
          <Upload />
          Add files
        </Button>
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

          <ZipExtractOptions count={zipCount} value={zipPrefs} onChange={setZipPrefs} />

          {imageFiles.length > 0 ? (
            <div className="space-y-1">
              <div className="text-sm font-medium">Main image (optional)</div>
              <select
                className="h-10 w-full rounded-lg border border-input bg-card px-3 text-sm"
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
