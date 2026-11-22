import * as React from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Box, ChevronDown, ChevronLeft, ChevronRight, Download, EyeOff, Image as ImageIcon, MoreHorizontal, Pencil, Rotate3d, Star, Trash2, File as FileIcon, FileArchive } from "lucide-react";

import type { components } from "@/gen/openapi";
import { apiGet } from "@/lib/api";
import { formatBytes, timeAgo } from "@/lib/format";
import { libraryUrl, projectDownloadUrl, thumbUrl } from "@/lib/media";
import { sliceable } from "@/lib/slicers";
import { OpenFileInSlicer, OpenProjectInSlicer } from "@/components/open-in-slicer";
import { ModelViewer } from "@/components/model-viewer";
import { viewerFormat } from "@/lib/viewer";
import { useTrash } from "@/lib/trash";
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
import { useDeleteAsset, useDeleteProject, usePatchProject } from "@/lib/project-mutations";
import { FavouriteButton } from "@/components/favourite-button";
import { ProjectTagsEditor } from "@/components/project-tags-editor";
import { ProjectCollectionsEditor } from "@/components/project-collections-editor";
import { MediaImage } from "@/components/media-image";
import { ZipContentsPanel } from "@/components/zip-contents";
import { ModelPlaceholder } from "@/components/model-placeholder";
import { MicroLabel, PageBody } from "@/components/layout";
import { Markdown } from "@/components/markdown";

type ProjectDetail = components["schemas"]["ProjectDetailResponse"];
type ProjectAsset = components["schemas"]["ProjectAssetResponse"];
type PatchProjectRequest = { name?: string; description?: string; main_image_id?: string | null };

function fileExt(path: string) {
  const i = path.lastIndexOf(".");
  return i > 0 ? path.slice(i + 1).toUpperCase() : "";
}

function isZip(path: string) {
  return path.toLowerCase().endsWith(".zip");
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

  // All three update the page immediately and roll back with a toast if the server refuses.
  const patchM = usePatchProject(projectId);
  const deleteAssetM = useDeleteAsset(projectId);
  const deleteProjectM = useDeleteProject(projectId, project?.name ?? "project");
  const trashDays = useTrash().data?.retention_days ?? 0;

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
  const viewable = models.filter((m) => viewerFormat(m.file_path));
  // What the gallery shows: the picked image or model, else the cover, else the first model.
  const shown =
    [...images, ...viewable].find((a) => a.id === selectedImage) ??
    images.find((a) => a.id === project.main_image_id) ??
    images[0] ??
    viewable[0] ??
    null;
  const current = shown?.kind === "image" ? shown : null;


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
        <Gallery project={project} images={images} models={viewable} current={shown} onSelect={setSelectedImage} />

        <aside className="flex min-w-0 flex-[1_1_380px] flex-col gap-5">
          <div className="flex flex-col gap-2.5">
            <span className="font-mono text-xs tracking-[0.12em] text-signal">// PROJECT</span>
            <h1 className="font-display text-3xl leading-[1.05] font-semibold tracking-[-0.03em] break-words sm:text-[40px]">
              {project.name}
            </h1>
            <span className="font-mono text-xs break-all text-muted-foreground">library/{project.folder_path}/</span>
            {project.source_url ? (
              <span className="text-xs text-muted-foreground">
                Imported from{" "}
                <a href={project.source_url} target="_blank" rel="noopener noreferrer" className="text-foreground underline-offset-4 hover:underline">
                  {sourceLabel(project.source_url)}
                </a>
                {project.source_author ? ` · by ${project.source_author}` : ""}
                {project.source_license ? ` · ${project.source_license}` : ""}
              </span>
            ) : null}
          </div>

          <div className="flex flex-col gap-2.5">
            <ProjectTagsEditor projectId={project.id} tags={project.tags} />
            <ProjectCollectionsEditor projectId={project.id} collections={project.collections} />
          </div>

          <div className="flex flex-wrap gap-2">
            <FavouriteButton projectId={project.id} isFavourite={project.is_favourite} />
            <OpenProjectInSlicer
              files={models
                .filter((m) => sliceable(m.file_path))
                .map((m) => ({ url: libraryUrl(project.folder_path, m.file_path), name: fileName(m.file_path) }))}
            />
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
                  Remove project…
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>

          {project.description.trim() ? (
            <Markdown>{project.description}</Markdown>
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

        </aside>
      </div>

      <FilesTable
        project={project}
        onView={(id) => {
          setSelectedImage(id);
          window.scrollTo({ top: 0, behavior: "smooth" });
        }}
        onSetCover={(id) => patchM.mutate({ main_image_id: id })}
        onDelete={(id) => deleteAssetM.mutate(id)}
      />

      <EditProjectDialog
        open={editing}
        onOpenChange={setEditing}
        project={project}
        images={images}
        onSave={(payload) => {
          patchM.mutate(payload);
          setEditing(false);
        }}
      />

      <Dialog open={confirmDelete} onOpenChange={setConfirmDelete}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Remove “{project.name}”?</DialogTitle>
            <DialogDescription>
              Choose what happens to the folder <span className="font-mono">{project.folder_path}</span>.
            </DialogDescription>
          </DialogHeader>
          <div className="grid gap-2.5">
            <button
              type="button"
              className="group rounded-xl border p-3.5 text-left transition-colors hover:border-line-strong hover:bg-secondary/60 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
              onClick={() => {
                deleteProjectM.mutate({ keepFiles: true });
                navigate("/");
              }}
            >
              <div className="flex items-center gap-2 font-medium">
                <EyeOff className="size-4 text-muted-foreground" />
                Remove from LiMa
              </div>
              <p className="mt-1 text-sm text-muted-foreground">
                The files stay on disk and sync stops picking the folder up. Its tags, collections and description are
                dropped. Un-ignore it from Sync → Ignored folders to index it again.
              </p>
            </button>
            <button
              type="button"
              className="group rounded-xl border border-destructive/40 p-3.5 text-left transition-colors hover:bg-destructive/10 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
              onClick={() => {
                deleteProjectM.mutate({ keepFiles: false });
                navigate("/");
              }}
            >
              <div className="flex items-center gap-2 font-medium text-destructive">
                <Trash2 className="size-4" />
                {trashDays > 0 ? "Move to trash" : "Delete files"}
              </div>
              <p className="mt-1 text-sm text-muted-foreground">
                {trashDays > 0
                  ? `The folder and its ${project.assets.length} file${project.assets.length === 1 ? "" : "s"} go to LiMa’s trash. Restore them, with tags and collections, from Sync → Trash for ${trashDays} days.`
                  : `Removes the folder and its ${project.assets.length} file${project.assets.length === 1 ? "" : "s"} from disk. It can’t be undone.`}
              </p>
            </button>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmDelete(false)}>
              Cancel
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
  models,
  current,
  onSelect,
}: {
  project: ProjectDetail;
  images: ProjectAsset[];
  /** Models the 3D viewer can show. */
  models: ProjectAsset[];
  current: ProjectAsset | null;
  onSelect: (id: string) => void;
}) {
  const items = [...images, ...models];
  const index = current ? items.findIndex((a) => a.id === current.id) : -1;
  const step = (d: number) => {
    if (items.length < 2) return;
    onSelect(items[(index + d + items.length) % items.length].id);
  };
  const isModel = current?.kind === "model";
  const format = current && isModel ? viewerFormat(current.file_path) : null;
  const label = isModel
    ? `3D ${models.indexOf(current!) + 1} / ${models.length}`
    : `IMG ${images.indexOf(current!) + 1} / ${images.length}`;

  return (
    <section aria-label="Images and 3D preview" className="flex min-w-0 flex-[999_1_560px] flex-col gap-3">
      <div
        className="corners bg-dots relative aspect-[16/11] overflow-hidden rounded-2xl border bg-card [--corner-inset:16px] [--corner-size:22px]"
        onKeyDown={(e) => {
          // Arrow keys orbit the 3D view; only step through images.
          if (isModel) return;
          if (e.key === "ArrowLeft") step(-1);
          if (e.key === "ArrowRight") step(1);
        }}
      >
        {current && format ? (
          <ModelViewer url={libraryUrl(project.folder_path, current.file_path)} format={format} sizeBytes={current.size_bytes} />
        ) : current ? (
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
            {label} · {fileName(current.file_path)}
          </span>
        ) : null}
        {items.length > 1 ? (
          <>
            <div className="absolute top-1/2 left-5 z-[3] -translate-y-1/2">
              <GalleryButton label="Previous" onClick={() => step(-1)}>
                <ChevronLeft />
              </GalleryButton>
            </div>
            <div className="absolute top-1/2 right-5 z-[3] -translate-y-1/2">
              <GalleryButton label="Next" onClick={() => step(1)}>
                <ChevronRight />
              </GalleryButton>
            </div>
          </>
        ) : null}
      </div>

      {items.length > 1 ? (
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
          {models.map((a) => (
            <button
              key={a.id}
              type="button"
              onClick={() => onSelect(a.id)}
              aria-label={`3D preview: ${fileName(a.file_path)}`}
              aria-pressed={a.id === current?.id}
              title={fileName(a.file_path)}
              className={cn(
                "bg-dots relative grid aspect-square cursor-pointer place-items-center overflow-hidden rounded-[10px] border bg-thumb text-signal",
                a.id === current?.id ? "border-2 border-signal" : "hover:border-line-strong",
              )}
            >
              <MediaImage src={thumbUrl(project.id, a.id)} alt="" fallbackLabel="" className="object-contain" />
              <span className="absolute right-1.5 bottom-1.5 left-1.5 z-[3] truncate rounded bg-black/60 px-1 py-px text-center font-mono text-[9px] text-white/85">
                3D · {fileExt(a.file_path)}
              </span>
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
  onView,
  onSetCover,
  onDelete,
}: {
  project: ProjectDetail;
  onView: (assetId: string) => void;
  onSetCover: (assetId: string) => void;
  onDelete: (assetId: string) => void;
}) {
  const [confirm, setConfirm] = React.useState<ProjectAsset | null>(null);
  const [opened, setOpened] = React.useState<Set<string>>(new Set());
  const toggle = (id: string) =>
    setOpened((s) => {
      const next = new Set(s);
      if (!next.delete(id)) next.add(id);
      return next;
    });

  return (
    <section aria-labelledby="files-h" className="flex flex-col gap-3 pt-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 id="files-h" className="font-display text-[22px] font-semibold">
          Files <span className="font-mono text-[13px] font-normal text-muted-foreground">{project.assets.length}</span>
        </h2>
        {project.assets.length > 0 ? (
          <Button asChild variant="outline">
            <a href={projectDownloadUrl(project.id)} download>
              <Download />
              Download all
              <span className="font-mono text-xs text-muted-foreground">
                zip · {formatBytes(project.assets.reduce((n, a) => n + a.size_bytes, 0))}
              </span>
            </a>
          </Button>
        ) : null}
      </div>
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
                const zip = isZip(a.file_path);
                const open = zip && opened.has(a.id);
                const Icon = a.kind === "model" ? Box : a.kind === "image" ? ImageIcon : zip ? FileArchive : FileIcon;
                return (
                  <React.Fragment key={a.id}>
                    <tr className="hover:bg-secondary/60">
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
                          {zip ? (
                            <Button
                              variant="outline"
                              size="icon-sm"
                              onClick={() => toggle(a.id)}
                              aria-expanded={open}
                              aria-label={`${open ? "Hide" : "Show"} what's inside ${a.file_path}`}
                              title={open ? "Hide contents" : "Show contents"}
                            >
                              <ChevronDown className={cn("transition-transform", open && "rotate-180")} />
                            </Button>
                          ) : null}
                          {a.kind === "model" && viewerFormat(a.file_path) ? (
                            <Button variant="outline" size="icon-sm" onClick={() => onView(a.id)} aria-label={`View ${a.file_path} in 3D`} title="View in 3D">
                              <Rotate3d />
                            </Button>
                          ) : null}
                          {a.kind === "model" && sliceable(a.file_path) ? (
                            <OpenFileInSlicer file={{ url, name: fileName(a.file_path) }} />
                          ) : null}
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
                    {open ? (
                      <tr>
                        <td colSpan={5} className="border-b p-0">
                          <ZipContentsPanel projectId={project.id} assetId={a.id} />
                        </td>
                      </tr>
                    ) : null}
                  </React.Fragment>
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
  onSave,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  project: ProjectDetail;
  images: ProjectAsset[];
  onSave: (payload: PatchProjectRequest) => void;
}) {
  const [name, setName] = React.useState(project.name);
  const [description, setDescription] = React.useState(project.description);
  const [descMode, setDescMode] = React.useState<"write" | "preview">("write");
  const [cover, setCover] = React.useState(project.main_image_id ?? "");

  React.useEffect(() => {
    if (!open) return;
    setName(project.name);
    setDescription(project.description);
    setDescMode("write");
    setCover(project.main_image_id ?? "");
  }, [open, project]);

  const canSave = name.trim().length > 0;

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
            <DialogDescription className="sr-only">Change the project's name, description and cover image.</DialogDescription>
          </DialogHeader>
          <div className="space-y-1.5">
            <label htmlFor="project-name" className="text-sm font-medium">
              Name
            </label>
            <Input id="project-name" value={name} onChange={(e) => setName(e.target.value)} autoFocus />
          </div>
          <div className="space-y-1.5">
            <div className="flex items-center justify-between gap-2">
              <label htmlFor="project-description" className="text-sm font-medium">
                Description <span className="font-normal text-muted-foreground">(Markdown)</span>
              </label>
              <div role="group" aria-label="Description view" className="flex rounded-md border border-input p-0.5 text-xs">
                {(["write", "preview"] as const).map((m) => (
                  <button
                    key={m}
                    type="button"
                    aria-pressed={descMode === m}
                    onClick={() => setDescMode(m)}
                    className={cn(
                      "cursor-pointer rounded px-2 py-0.5 capitalize",
                      descMode === m ? "bg-secondary text-foreground" : "text-muted-foreground hover:text-foreground",
                    )}
                  >
                    {m}
                  </button>
                ))}
              </div>
            </div>
            {descMode === "write" ? (
              <Textarea
                id="project-description"
                rows={8}
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder="Print settings, notes, where it came from… **bold**, - lists and [links](https://…) work."
                className="font-mono text-[13px]"
              />
            ) : (
              <div className="max-h-72 min-h-[9.5rem] overflow-auto rounded-lg border border-input p-3">
                {description.trim() ? <Markdown>{description}</Markdown> : <span className="text-sm text-dim">Nothing to preview.</span>}
              </div>
            )}
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

/** "Printables", "MakerWorld" or the host name of an import source URL. */
function sourceLabel(url: string): string {
  try {
    const host = new URL(url).hostname.replace(/^www\./, "");
    if (host === "printables.com") return "Printables";
    if (host === "makerworld.com") return "MakerWorld";
    return host;
  } catch {
    return url;
  }
}
