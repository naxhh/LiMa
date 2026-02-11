import * as React from "react";
import { useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { apiGet, apiJson, apiJsonNoResponse, apiNoBody, ApiError } from "@/lib/api";
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
import { ImportAssetsDialog } from "@/components/import-assets-dialog";

import { MoreHorizontal } from "lucide-react";

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { MediaCard } from "../components/media-card";



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
      apiJsonNoResponse<void>("POST", `/projects/${projectId}/import`, payload),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["project", projectId] });
      await qc.invalidateQueries({ queryKey: ["projects"] });
    },
  });

  const setMainImageM = useMutation({
    mutationFn: (assetId: string) =>
      apiJson<void>("PATCH", `/projects/${projectId}`, { main_image_id: assetId }),
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
          <EditProjectDialog
            project={project}
            imageAssets={imageAssets}
            isLoading={patchM.isPending}
            error={patchM.error}
            onSave={(payload) => patchM.mutate(payload)}
          />
          <ImportAssetsDialog
            projectId={projectId}
            onImported={async () => {
              await qc.invalidateQueries({ queryKey: ["project", projectId] });
              await qc.invalidateQueries({ queryKey: ["projects"] });
            }}
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
      <div className="space-y-3">
        <div className="flex items-center justify-between gap-3">
          <div className="text-lg font-semibold">Assets</div>
          <div className="text-xs text-muted-foreground">
            {project.assets.length} item(s)
          </div>
        </div>

        {project.assets.length === 0 ? (
          <div className="text-sm text-muted-foreground">No assets</div>
        ) : (
          <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-3">
            {project.assets.map((a) => {
              const isMain = project.main_image_id === a.id;

              return (
                <MediaCard
                  title={a.file_path}
                  subtitle={`${a.kind} · ${formatBytes(a.size_bytes)}`}
                  meta={isMain ? "Main image" : null}
                  placeholder={a.kind === "image" ? "image preview" : a.kind === "model" ? "3D preview" : "file"}
                  chips={[
                    { label: a.kind, variant: "secondary" },
                    ...(isMain ? [{ label: "Main", variant: "default" as const }] : []),
                  ]}
                  actions={[
                    ...(a.kind === "image"
                      ? [
                          {
                            label: "Set as main image",
                            onClick: () => setMainImageM.mutate(a.id),
                            disabled: isMain || setMainImageM.isPending,
                          },
                          { label: "sep", onClick: () => {}, separatorBefore: true, disabled: true }, // (see note)
                        ]
                      : []),
                    {
                      label: "Delete",
                      destructive: true,
                      onClick: () => deleteAssetM.mutate(a.id),
                      disabled: deleteAssetM.isPending,
                      separatorBefore: a.kind === "image",
                    },
                  ].filter((x) => x.label !== "sep")}
                />
              );
            })}
          </div>
        )}

        {deleteAssetM.isError ? (
          <div className="text-sm text-destructive">
            Delete failed: {getApiErrorMessage(deleteAssetM.error)}
          </div>
        ) : null}

        {setMainImageM.isError ? (
          <div className="text-sm text-destructive">
            Set main image failed: {getApiErrorMessage(setMainImageM.error)}
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
