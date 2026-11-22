import * as React from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Box, ChevronLeft, ChevronRight, Download, Image as ImageIcon, MoreHorizontal, Pencil, Star, Trash2, File as FileIcon } from "lucide-react";

import type { components } from "@/gen/openapi";
import { apiGet, apiJson, apiNoBody } from "@/lib/api";
import { formatBytes, timeAgo } from "@/lib/format";
import { libraryUrl, thumbUrl } from "@/lib/media";
import { apiErrorMessage } from "@/lib/tags";
import { cn } from "@/lib/utils";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ImportAssetsDialog } from "@/components/import-assets-dialog";
import { ProjectTagsEditor } from "@/components/project-tags-editor";
import { MediaImage } from "@/components/media-image";
import { ModelPlaceholder } from "@/components/model-placeholder";
import { MicroLabel, PageBody } from "@/components/layout";

type ProjectDetail = components["schemas"]["ProjectDetailResponse"];
type ProjectAsset = components["schemas"]["ProjectAssetResponse"];
type PatchProjectRequest = { name?: string; description?: string; main_image_id?: string | null };

function fileExt(path: string) {
  const i = path.lastIndexOf(".");
  return i > 0 ? path.slice(i + 1).toUpperCase() : "";
}

function fileName(path: string) {
  return path.split("/").pop() ?? path;
}

export function ProjectDetailPage() {
  const { projectId } = useParams();
  if (!projectId) return <PageBody className="py-10">Missing project id</PageBody>;
  return <ProjectDetailInner projectId={projectId} />;
}

function ProjectDetailInner({ projectId }: { projectId: string }) {
  const qc = useQueryClient();
  const navigate = useNavigate();

  const projectQ = useQuery({
    queryKey: ["project", projectId],
    queryFn: () => apiGet<ProjectDetail>(`/projects/${projectId}`),
  });
  const project = projectQ.data;

  const refresh = async () => {
    await Promise.all([
      qc.invalidateQueries({ queryKey: ["project", projectId] }),
      qc.invalidateQueries({ queryKey: ["projects"] }),
    ]);
  };

  const patchM = useMutation({
    mutationFn: (payload: PatchProjectRequest) => apiJson<void>("PATCH", `/projects/${projectId}`, payload),
    onSuccess: refresh,
  });
  const deleteAssetM = useMutation({
    mutationFn: (assetId: string) => apiNoBody("DELETE", `/projects/${projectId}/assets/${assetId}`),
    onSuccess: refresh,
  });
  const deleteProjectM = useMutation({
    mutationFn: () => apiNoBody("DELETE", `/projects/${projectId}`),
    onSuccess: async () => {
      qc.removeQueries({ queryKey: ["project", projectId] });
      await qc.invalidateQueries({ queryKey: ["projects"] });
      navigate("/");
    },
  });

  const [editing, setEditing] = React.useState(false);
  const [confirmDelete, setConfirmDelete] = React.useState(false);
  const [selectedImage, setSelectedImage] = React.useState<string | null>(null);

  if (projectQ.isLoading) {
    return (
      <PageBody className="py-6">
        <div className="bg-dots aspect-[16/9] max-h-[60vh] animate-pulse rounded-2xl border bg-thumb" />
      </PageBody>
    );
  }
  if (projectQ.isError || !project) {
    return (
      <PageBody className="py-10">
        <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-4 text-sm text-destructive">
          Could not load the project: {apiErrorMessage(projectQ.error)}
        </div>
        <Link to="/" className="mt-4 inline-block text-sm text-signal">
          ← Back to the library
        </Link>
      </PageBody>
    );
  }

  const images = project.assets.filter((a) => a.kind === "image");
  const models = project.assets.filter((a) => a.kind === "model");
  const totalBytes = project.assets.reduce((n, a) => n + a.size_bytes, 0);
  const current =
    images.find((a) => a.id === selectedImage) ??
    images.find((a) => a.id === project.main_image_id) ??
    images[0] ??
    null;

  const mutationError = patchM.error ?? deleteAssetM.error ?? deleteProjectM.error;

  return (
    <PageBody className="flex flex-col gap-5 pt-5 pb-16">
      <nav aria-label="Breadcrumb" className="flex min-w-0 items-center gap-2 font-mono text-xs text-muted-foreground">
        <Link to="/" className="hover:text-signal">
          library
        </Link>
        <span className="text-dim">/</span>
        <span className="truncate text-foreground">{project.folder_path}</span>
      </nav>

      <div className="flex flex-wrap items-start gap-8">
        <Gallery
          project={project}
          images={images}
          current={current}
          onSelect={setSelectedImage}
        />

        <aside className="flex min-w-0 flex-[1_1_380px] flex-col gap-5">
          <div className="flex flex-col gap-2.5">
            <span className="font-mono text-xs tracking-[0.12em] text-signal">// PROJECT</span>
            <h1 className="font-display text-3xl leading-[1.05] font-semibold tracking-[-0.03em] break-words sm:text-[40px]">
              {project.name}
            </h1>
            <span className="font-mono text-xs break-all text-muted-foreground">library/{project.folder_path}/</span>
          </div>

          <ProjectTagsEditor projectId={project.id} tags={project.tags} />

          <div className="flex flex-wrap gap-2">
            <ImportAssetsDialog projectId={project.id} onImported={refresh} />
            <Button variant="outline" onClick={() => setEditing(true)}>
              <Pencil />
              Edit
            </Button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="outline" size="icon" aria-label="More actions">
                  <MoreHorizontal />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                {current && current.id !== project.main_image_id ? (
                  <DropdownMenuItem onClick={() => patchM.mutate({ main_image_id: current.id })}>
                    <Star />
                    Use this image as cover
                  </DropdownMenuItem>
                ) : null}
                {current ? (
                  <DropdownMenuItem asChild>
                    <a href={libraryUrl(project.folder_path, current.file_path)} target="_blank" rel="noopener">
                      <ImageIcon />
                      Open image in new tab
                    </a>
                  </DropdownMenuItem>
                ) : null}
                {current ? <DropdownMenuSeparator /> : null}
                <DropdownMenuItem variant="destructive" onClick={() => setConfirmDelete(true)}>
                  <Trash2 />
                  Delete project
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>

          {project.description.trim() ? (
            <p className="text-[15px] leading-relaxed whitespace-pre-line text-muted-foreground">{project.description}</p>
          ) : (
            <button
              type="button"
              onClick={() => setEditing(true)}
              className="cursor-pointer self-start text-sm text-dim hover:text-foreground"
            >
              + Add a description
            </button>
          )}

          <dl className="grid grid-cols-2 overflow-hidden rounded-xl border">
            <Spec label="Files" value={`${project.assets.length} · ${models.length} model${models.length === 1 ? "" : "s"}`} className="border-r border-b" />
            <Spec label="Size" value={formatBytes(totalBytes)} className="border-b" />
            <Spec label="Added" value={new Date(project.created_at).toLocaleDateString()} className="border-r" />
            <Spec label="Last scan" value={timeAgo(project.last_scanned_at)} />
          </dl>

          {mutationError ? (
            <div className="text-sm text-destructive">Something failed: {apiErrorMessage(mutationError)}</div>
          ) : null}
        </aside>
      </div>

      <FilesTable
        project={project}
        onSetCover={(id) => patchM.mutate({ main_image_id: id })}
        onDelete={(id) => deleteAssetM.mutate(id)}
        deleting={deleteAssetM.isPending ? deleteAssetM.variables : undefined}
      />

      <EditProjectDialog
        open={editing}
        onOpenChange={setEditing}
        project={project}
        images={images}
        saving={patchM.isPending}
        onSave={(payload) => patchM.mutate(payload, { onSuccess: () => setEditing(false) })}
      />

      <Dialog open={confirmDelete} onOpenChange={setConfirmDelete}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete “{project.name}”?</DialogTitle>
            <DialogDescription>
              This removes the project folder <span className="font-mono">{project.folder_path}</span> and its{" "}
              {project.assets.length} file{project.assets.length === 1 ? "" : "s"} from the library. It can’t be undone.
            </DialogDescription>
          </DialogHeader>
          {deleteProjectM.isError ? (
            <div className="text-sm text-destructive">{apiErrorMessage(deleteProjectM.error)}</div>
          ) : null}
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmDelete(false)}>
              Cancel
            </Button>
            <Button variant="destructive" disabled={deleteProjectM.isPending} onClick={() => deleteProjectM.mutate()}>
              Delete project
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </PageBody>
  );
}

function Spec({ label, value, className }: { label: string; value: string; className?: string }) {
  return (
    <div className={cn("px-3.5 py-3", className)}>
      <dt>
        <MicroLabel>{label}</MicroLabel>
      </dt>
      <dd className="mt-1 font-mono text-[15px]">{value}</dd>
    </div>
  );
}

function Gallery({
  project,
  images,
  current,
  onSelect,
}: {
  project: ProjectDetail;
  images: ProjectAsset[];
  current: ProjectAsset | null;
  onSelect: (id: string) => void;
}) {
  const index = current ? images.findIndex((a) => a.id === current.id) : -1;
  const step = (d: number) => {
    if (images.length < 2) return;
    onSelect(images[(index + d + images.length) % images.length].id);
  };

  return (
    <section aria-label="Images" className="flex min-w-0 flex-[999_1_560px] flex-col gap-3">
      <div
        className="corners bg-dots relative aspect-[16/11] overflow-hidden rounded-2xl border bg-card [--corner-inset:16px] [--corner-size:22px]"
        onKeyDown={(e) => {
          if (e.key === "ArrowLeft") step(-1);
          if (e.key === "ArrowRight") step(1);
        }}
      >
        {current ? (
          <img
            key={current.id}
            src={libraryUrl(project.folder_path, current.file_path)}
            alt={fileName(current.file_path)}
            className="absolute inset-0 h-full w-full object-contain"
          />
        ) : (
          <ModelPlaceholder label="no images in this project" />
        )}
        {current ? (
          <span className="absolute top-4 left-12 z-[3] max-w-[70%] truncate rounded bg-black/60 px-2 py-0.5 font-mono text-[11px] text-white/85">
            IMG {index + 1} / {images.length} · {fileName(current.file_path)}
          </span>
        ) : null}
        {images.length > 1 ? (
          <div className="absolute top-1/2 right-5 z-[3] flex -translate-y-1/2 flex-col gap-2">
            <GalleryButton label="Previous image" onClick={() => step(-1)}>
              <ChevronLeft />
            </GalleryButton>
            <GalleryButton label="Next image" onClick={() => step(1)}>
              <ChevronRight />
            </GalleryButton>
          </div>
        ) : null}
      </div>

      {images.length > 1 ? (
        <div className="grid grid-cols-4 gap-2.5 sm:grid-cols-6">
          {images.map((a, i) => (
            <button
              key={a.id}
              type="button"
              onClick={() => onSelect(a.id)}
              aria-label={`Image ${i + 1}: ${fileName(a.file_path)}`}
              aria-pressed={a.id === current?.id}
              className={cn(
                "relative aspect-square cursor-pointer overflow-hidden rounded-[10px] border bg-thumb",
                a.id === current?.id ? "border-2 border-signal" : "hover:border-line-strong",
              )}
            >
              <MediaImage src={thumbUrl(project.id, a.id)} alt="" />
              {a.id === project.main_image_id ? (
                <span className="absolute bottom-1.5 left-1.5 rounded bg-signal px-1.5 py-px font-mono text-[9px] tracking-[0.08em] text-primary-foreground">
                  COVER
                </span>
              ) : null}
            </button>
          ))}
        </div>
      ) : null}
    </section>
  );
}

function GalleryButton({ label, onClick, children }: { label: string; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      className="grid size-10 cursor-pointer place-items-center rounded-full border border-white/15 bg-black/55 text-white backdrop-blur hover:bg-black/75 [&_svg]:size-4"
    >
      {children}
    </button>
  );
}

function FilesTable({
  project,
  onSetCover,
  onDelete,
  deleting,
}: {
  project: ProjectDetail;
  onSetCover: (assetId: string) => void;
  onDelete: (assetId: string) => void;
  deleting?: string;
}) {
  const [confirm, setConfirm] = React.useState<ProjectAsset | null>(null);

  return (
    <section aria-labelledby="files-h" className="flex flex-col gap-3 pt-3">
      <h2 id="files-h" className="font-display text-[22px] font-semibold">
        Files <span className="font-mono text-[13px] font-normal text-muted-foreground">{project.assets.length}</span>
      </h2>
      {project.assets.length === 0 ? (
        <div className="rounded-xl border border-dashed p-6 text-sm text-muted-foreground">
          No files yet. Use “Add files” to upload some.
        </div>
      ) : (
        <div className="overflow-x-auto rounded-xl border">
          <table className="w-full min-w-[640px] border-collapse text-left">
            <thead>
              <tr className="font-mono text-[10px] tracking-[0.1em] text-dim">
                <th className="border-b px-4 py-2.5 font-medium">NAME</th>
                <th className="border-b px-4 py-2.5 font-medium">TYPE</th>
                <th className="border-b px-4 py-2.5 text-right font-medium">SIZE</th>
                <th className="border-b px-4 py-2.5 font-medium">BLAKE3</th>
                <th className="border-b px-4 py-2.5">
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {project.assets.map((a) => {
                const url = libraryUrl(project.folder_path, a.file_path);
                const Icon = a.kind === "model" ? Box : a.kind === "image" ? ImageIcon : FileIcon;
                return (
                  <tr key={a.id} className={cn("hover:bg-secondary/60", deleting === a.id && "opacity-50")}>
                    <td className="border-b px-4 py-2.5">
                      <a href={url} download={fileName(a.file_path)} className="flex min-w-0 items-center gap-2.5 text-sm hover:text-signal">
                        <span
                          className={cn(
                            "grid size-[30px] shrink-0 place-items-center rounded-lg",
                            a.kind === "model" ? "bg-signal-soft text-signal" : "bg-secondary text-info",
                          )}
                        >
                          <Icon className="size-[15px]" />
                        </span>
                        <span className="truncate">{a.file_path}</span>
                        {a.id === project.main_image_id ? (
                          <span className="rounded bg-signal-soft px-1.5 py-px font-mono text-[9px] tracking-[0.08em] text-signal">COVER</span>
                        ) : null}
                      </a>
                    </td>
                    <td className="border-b px-4 py-2.5 font-mono text-xs text-muted-foreground">
                      {a.kind} · {fileExt(a.file_path) || "—"}
                    </td>
                    <td className="border-b px-4 py-2.5 text-right font-mono text-xs">{formatBytes(a.size_bytes)}</td>
                    <td className="border-b px-4 py-2.5 font-mono text-xs text-dim" title={a.file_hash ?? "not hashed yet"}>
                      {a.file_hash ? `${a.file_hash.slice(0, 4)}…${a.file_hash.slice(-4)}` : "pending"}
                    </td>
                    <td className="border-b px-3 py-1.5">
                      <div className="flex justify-end gap-1">
                        <Button variant="outline" size="icon-sm" asChild>
                          <a href={url} download={fileName(a.file_path)} aria-label={`Download ${a.file_path}`}>
                            <Download />
                          </a>
                        </Button>
                        <DropdownMenu>
                          <DropdownMenuTrigger asChild>
                            <Button variant="ghost" size="icon-sm" aria-label={`More actions for ${a.file_path}`}>
                              <MoreHorizontal />
                            </Button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent align="end">
                            {a.kind === "image" && a.id !== project.main_image_id ? (
                              <DropdownMenuItem onClick={() => onSetCover(a.id)}>
                                <Star />
                                Use as cover
                              </DropdownMenuItem>
                            ) : null}
                            <DropdownMenuItem variant="destructive" onClick={() => setConfirm(a)}>
                              <Trash2 />
                              Delete file
                            </DropdownMenuItem>
                          </DropdownMenuContent>
                        </DropdownMenu>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      <Dialog open={confirm !== null} onOpenChange={(o) => !o && setConfirm(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete {confirm ? fileName(confirm.file_path) : "file"}?</DialogTitle>
            <DialogDescription>The file is removed from the project folder on disk.</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirm(null)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                if (confirm) onDelete(confirm.id);
                setConfirm(null);
              }}
            >
              Delete file
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </section>
  );
}

function EditProjectDialog({
  open,
  onOpenChange,
  project,
  images,
  saving,
  onSave,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  project: ProjectDetail;
  images: ProjectAsset[];
  saving: boolean;
  onSave: (payload: PatchProjectRequest) => void;
}) {
  const [name, setName] = React.useState(project.name);
  const [description, setDescription] = React.useState(project.description);
  const [cover, setCover] = React.useState(project.main_image_id ?? "");

  React.useEffect(() => {
    if (!open) return;
    setName(project.name);
    setDescription(project.description);
    setCover(project.main_image_id ?? "");
  }, [open, project]);

  const canSave = name.trim().length > 0 && !saving;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <form
          className="space-y-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (canSave) onSave({ name: name.trim(), description, main_image_id: cover || null });
          }}
        >
          <DialogHeader>
            <DialogTitle>Edit project</DialogTitle>
          </DialogHeader>
          <div className="space-y-1.5">
            <label htmlFor="project-name" className="text-sm font-medium">
              Name
            </label>
            <Input id="project-name" value={name} onChange={(e) => setName(e.target.value)} autoFocus />
          </div>
          <div className="space-y-1.5">
            <label htmlFor="project-description" className="text-sm font-medium">
              Description
            </label>
            <Textarea
              id="project-description"
              rows={5}
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Print settings, notes, where it came from…"
            />
          </div>
          {images.length > 0 ? (
            <fieldset className="space-y-2">
              <legend className="mb-2 text-sm font-medium">Cover image</legend>
              <div className="grid grid-cols-5 gap-2">
                {images.map((a) => (
                  <button
                    key={a.id}
                    type="button"
                    onClick={() => setCover(a.id)}
                    aria-pressed={cover === a.id}
                    aria-label={fileName(a.file_path)}
                    className={cn(
                      "relative aspect-square cursor-pointer overflow-hidden rounded-lg border bg-thumb",
                      cover === a.id && "border-2 border-signal",
                    )}
                  >
                    <MediaImage src={thumbUrl(project.id, a.id)} alt="" />
                  </button>
                ))}
              </div>
            </fieldset>
          ) : null}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={!canSave}>
              Save
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
