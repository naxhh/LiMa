import * as React from "react";
import { useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { apiGet, apiJson, apiNoBody, ApiError } from "@/lib/api";
import { formatBytes } from "@/lib/format";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";

type ProjectAsset = {
  id: string;
  file_path: string;
  kind: string;
  size_bytes: number;
};

type ProjectTag = {
  id: string;
  name: string;
  color: string;
};

type ProjectDetailResponse = {
  id: string;
  name: string;
  description: string;
  main_image_id: string | null;
  created_at: string;
  updated_at: string;
  last_scanned_at: string | null;
  assets: ProjectAsset[];
  tags: ProjectTag[];
};

type PatchProjectRequest = {
  name?: string | null;
  description?: string | null;
  main_image_id?: string | null;
};

type ImportProjectRequest = {
  bundle_id: string;
  new_main_image?: string | null; // filename (file_path), not asset id
};

function getApiErrorMessage(e: unknown): string {
  if (e instanceof ApiError) {
    const body = e.body as any;
    const msg = body?.error?.message || body?.message;
    return msg ? `${msg}` : `Request failed (${e.status})`;
  }
  return "Request failed";
}

export function ProjectDetailPage() {
  const { projectId } = useParams();
  if (!projectId) return <div className="p-6">Missing project id</div>;

  const qc = useQueryClient();

  const projectQ = useQuery({
    queryKey: ["project", projectId],
    queryFn: () => apiGet<ProjectDetailResponse>(`/projects/${projectId}`),
  });

  const project = projectQ.data;

  const patchM = useMutation({
    mutationFn: (payload: PatchProjectRequest) =>
      apiJson<void>("PATCH", `/projects/${projectId}`, payload),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["project", projectId] });
      await qc.invalidateQueries({ queryKey: ["projects"] });
    },
  });

  const deleteAssetM = useMutation({
    mutationFn: (assetId: string) =>
      apiNoBody("DELETE", `/projects/${projectId}/assets/${assetId}`),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["project", projectId] });
    },
  });

  const importM = useMutation({
    mutationFn: (payload: ImportProjectRequest) =>
      apiJson<void>("POST", `/projects/${projectId}/import`, payload),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["project", projectId] });
      await qc.invalidateQueries({ queryKey: ["projects"] });
    },
  });

  if (projectQ.isLoading) return <div className="p-6">Loading…</div>;
  if (projectQ.isError) return <div className="p-6">Error: {getApiErrorMessage(projectQ.error)}</div>;
  if (!project) return <div className="p-6">Not found</div>;

  const imageAssets = project.assets.filter((a) => a.kind === "image");
  const mainImage = project.main_image_id
    ? project.assets.find((a) => a.id === project.main_image_id)
    : null;

  return (
    <div className="p-6 space-y-6">
      {/* Header */}
      <div className="flex items-start justify-between gap-4">
        <div className="space-y-2">
          <div className="text-2xl font-semibold">{project.name}</div>
          <div className="text-sm text-muted-foreground">
            {project.description?.trim() ? project.description : "—"}
          </div>
          <div className="text-xs text-muted-foreground">
            Updated: {project.updated_at} · Created: {project.created_at}
          </div>

          {/* Tags */}
          <div className="flex flex-wrap gap-2 pt-2">
            {project.tags.length === 0 ? (
              <span className="text-sm text-muted-foreground">No tags</span>
            ) : (
              project.tags.map((t) => (
                <Badge
                  key={t.id}
                  style={{ backgroundColor: t.color, color: "#fff", border: "none" }}
                >
                  {t.name}
                </Badge>
              ))
            )}
          </div>
        </div>

        <div className="flex items-center gap-2">
          <ImportBundleDialog
            isLoading={importM.isPending}
            error={importM.error}
            imageAssets={imageAssets}
            onSubmit={(payload) => importM.mutate(payload)}
          />
          <EditProjectDialog
            project={project}
            imageAssets={imageAssets}
            isLoading={patchM.isPending}
            error={patchM.error}
            onSave={(payload) => patchM.mutate(payload)}
          />
        </div>
      </div>

      {/* Main image preview (if known) */}
      {mainImage ? (
        <div className="rounded-lg border p-3">
          <div className="text-sm font-medium mb-2">Main image</div>
          <div className="text-sm text-muted-foreground">{mainImage.file_path}</div>
          {/* Real image serving later; placeholder for now */}
          <div className="mt-2 text-xs text-muted-foreground">
            (Image serving not wired yet)
          </div>
        </div>
      ) : null}

      {/* Assets */}
      <div className="space-y-2">
        <div className="text-lg font-semibold">Assets</div>

        {project.assets.length === 0 ? (
          <div className="text-sm text-muted-foreground">No assets</div>
        ) : (
          <div className="space-y-2">
            {project.assets.map((a) => (
              <div key={a.id} className="rounded-lg border p-3 flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <div className="font-medium truncate">{a.file_path}</div>
                  <div className="text-xs text-muted-foreground">
                    {a.kind} · {formatBytes(a.size_bytes)}
                  </div>
                </div>

                <Button
                  variant="destructive"
                  size="sm"
                  disabled={deleteAssetM.isPending}
                  onClick={() => deleteAssetM.mutate(a.id)}
                >
                  Delete
                </Button>
              </div>
            ))}
          </div>
        )}

        {deleteAssetM.isError ? (
          <div className="text-sm text-destructive">
            Delete failed: {getApiErrorMessage(deleteAssetM.error)}
          </div>
        ) : null}
      </div>
    </div>
  );
}

function EditProjectDialog(props: {
  project: ProjectDetailResponse;
  imageAssets: ProjectAsset[];
  isLoading: boolean;
  error: unknown;
  onSave: (payload: PatchProjectRequest) => void;
}) {
  const { project, imageAssets, isLoading, error, onSave } = props;

  const [open, setOpen] = React.useState(false);

  const [name, setName] = React.useState(project.name);
  const [description, setDescription] = React.useState(project.description ?? "");
  const [mainImageId, setMainImageId] = React.useState<string>(project.main_image_id ?? "");

  React.useEffect(() => {
    if (!open) return;
    setName(project.name);
    setDescription(project.description ?? "");
    setMainImageId(project.main_image_id ?? "");
  }, [open, project]);

  const canSave = name.trim().length > 0;

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="outline">Edit</Button>
      </DialogTrigger>

      <DialogContent>
        <DialogHeader>
          <DialogTitle>Edit project</DialogTitle>
        </DialogHeader>

        <div className="space-y-3">
          <div className="space-y-1">
            <div className="text-sm font-medium">Name</div>
            <Input value={name} onChange={(e) => setName(e.target.value)} />
          </div>

          <div className="space-y-1">
            <div className="text-sm font-medium">Description</div>
            <Textarea value={description} onChange={(e) => setDescription(e.target.value)} />
          </div>

          <div className="space-y-1">
            <div className="text-sm font-medium">Main image</div>
            <select
              className="w-full rounded-md border bg-background px-3 py-2 text-sm"
              value={mainImageId}
              onChange={(e) => setMainImageId(e.target.value)}
            >
              <option value="">(none)</option>
              {imageAssets.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.file_path}
                </option>
              ))}
            </select>
            <div className="text-xs text-muted-foreground">
              Uses asset id. (Will display once image serving is added.)
            </div>
          </div>

          {error ? (
            <div className="text-sm text-destructive">Save failed: {getApiErrorMessage(error)}</div>
          ) : null}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => setOpen(false)} disabled={isLoading}>
            Cancel
          </Button>
          <Button
            disabled={!canSave || isLoading}
            onClick={() => {
              onSave({
                name: name.trim(),
                description,
                main_image_id: mainImageId ? mainImageId : null,
              });
              setOpen(false);
            }}
          >
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ImportBundleDialog(props: {
  isLoading: boolean;
  error: unknown;
  imageAssets: ProjectAsset[]; // only for suggestion list UI
  onSubmit: (payload: ImportProjectRequest) => void;
}) {
  const { isLoading, error, imageAssets, onSubmit } = props;
  const [open, setOpen] = React.useState(false);

  const [bundleId, setBundleId] = React.useState("");
  const [newMainImage, setNewMainImage] = React.useState<string>("");

  React.useEffect(() => {
    if (!open) {
      setBundleId("");
      setNewMainImage("");
    }
  }, [open]);

  const canImport = bundleId.trim().length > 0;

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button>Import bundle</Button>
      </DialogTrigger>

      <DialogContent>
        <DialogHeader>
          <DialogTitle>Import bundle</DialogTitle>
        </DialogHeader>

        <div className="space-y-3">
          <div className="space-y-1">
            <div className="text-sm font-medium">Bundle id</div>
            <Input value={bundleId} onChange={(e) => setBundleId(e.target.value)} placeholder="uuid…" />
          </div>

          <div className="space-y-1">
            <div className="text-sm font-medium">Set main image (optional)</div>
            <Input
              value={newMainImage}
              onChange={(e) => setNewMainImage(e.target.value)}
              placeholder="filename in imported bundle (e.g. photo.jpg)"
            />
            <div className="text-xs text-muted-foreground">
              This is the *file name* that will be searched among imported assets.
            </div>
          </div>

          {imageAssets.length > 0 ? (
            <div className="text-xs text-muted-foreground">
              Existing images in project: {imageAssets.slice(0, 3).map((a) => a.file_path).join(", ")}
              {imageAssets.length > 3 ? "…" : ""}
            </div>
          ) : null}

          {error ? (
            <div className="text-sm text-destructive">Import failed: {getApiErrorMessage(error)}</div>
          ) : null}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => setOpen(false)} disabled={isLoading}>
            Cancel
          </Button>
          <Button
            disabled={!canImport || isLoading}
            onClick={() => {
              onSubmit({
                bundle_id: bundleId.trim(),
                new_main_image: newMainImage.trim() ? newMainImage.trim() : null,
              });
              setOpen(false);
            }}
          >
            Import
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
