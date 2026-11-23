import * as React from "react";
import { Link } from "react-router-dom";
import { Box, Check, Copy, EyeOff, FileBox, Image as ImageIcon, Loader2, Star, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { PageBody, PageHero } from "@/components/layout";
import { AdminNav } from "@/components/admin-nav";
import { adminIndex } from "@/lib/admin";
import {
  useClearDismissed,
  useDismissDuplicates,
  useDuplicates,
  useResolveDuplicates,
  type DuplicateGroup,
  type DuplicateProject,
  type RemoveMode,
  type SharedFileGroup,
} from "@/lib/duplicates";
import { formatBytes, formatCount, timeAgo } from "@/lib/format";
import { thumbUrl } from "@/lib/media";
import { apiErrorMessage } from "@/lib/tags";
import { useTrash } from "@/lib/trash";
import { cn } from "@/lib/utils";

/** Rows shown before "show all" in big groups. */
const PREVIEW_ROWS = 6;

export function AdminDuplicatesPage() {
  const q = useDuplicates();
  const clear = useClearDismissed();
  const d = q.data;
  const groups = d?.project_groups ?? [];
  const files = d?.shared_files ?? [];
  const copies = groups.reduce((n, g) => n + g.projects.length - 1, 0);

  return (
    <div>
      <PageHero
        index={adminIndex("/admin/duplicates")}
        label="Duplicates"
        title="Duplicates"
        subtitle={
          d && groups.length
            ? `${formatCount(groups.length, "group")} · ${formatCount(copies, "extra copy", "extra copies")} · ${formatBytes(d.reclaimable_bytes)} reclaimable`
            : "Projects with the same files · keep one and its copies' tags, collections and description move onto it"
        }
      />

      <PageBody className="flex flex-col gap-6 pt-7 pb-16">
        <AdminNav
          duplicatesCount={groups.length}
          actions={
            d && d.dismissed > 0 ? (
              <Button variant="outline" size="sm" disabled={clear.isPending} onClick={() => clear.mutate()}>
                <EyeOff />
                Show {formatCount(d.dismissed, "hidden group")}
              </Button>
            ) : null
          }
        />
        {q.isError ? <div className="text-sm text-destructive">{apiErrorMessage(q.error)}</div> : null}
        {q.isPending ? (
          <div className="flex items-center gap-2 font-mono text-xs text-muted-foreground">
            <Loader2 className="size-3.5 animate-spin" />
            comparing files
          </div>
        ) : null}
        {d && d.unhashed_files > 0 ? (
          <div className="rounded-xl border bg-card p-3 text-sm text-muted-foreground">
            {formatCount(d.unhashed_files, "file is", "files are")} still being fingerprinted by sync and can't be compared yet.
          </div>
        ) : null}

        {d && groups.length === 0 ? (
          <div className="bg-dots rounded-2xl border border-dashed p-10 text-center">
            <div className="font-display text-xl font-semibold">No duplicate projects</div>
            <p className="mt-2 text-sm text-muted-foreground">Projects whose model files are the same (or 80% the same) show up here.</p>
          </div>
        ) : null}

        {groups.map((g) => (
          <GroupCard key={g.key} group={g} />
        ))}

        {files.length > 0 ? <SharedFiles files={files} /> : null}
      </PageBody>
    </div>
  );
}

function GroupCard({ group }: { group: DuplicateGroup }) {
  const [keepId, setKeepId] = React.useState(group.projects[0].id);
  const [expanded, setExpanded] = React.useState(false);
  const [confirming, setConfirming] = React.useState(false);
  const dismiss = useDismissDuplicates();
  const keep = group.projects.find((p) => p.id === keepId) ?? group.projects[0];
  const others = group.projects.filter((p) => p.id !== keep.id);
  const shown = expanded ? group.projects : group.projects.slice(0, PREVIEW_ROWS);
  const name = React.useId();

  return (
    <section aria-label={`Duplicates of ${group.projects[0].name}`} className="overflow-hidden rounded-2xl border bg-card">
      <header className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b px-4.5 py-3">
        <Copy className="size-4 text-dim" />
        <span className="font-medium">{formatCount(group.projects.length, "copy", "copies")}</span>
        <span className="font-mono text-xs text-muted-foreground">
          {group.identical ? "identical model files" : `${formatCount(group.shared_models, "model file")} in common`} · {formatBytes(group.reclaimable_bytes)}{" "}
          reclaimable
        </span>
        <Button variant="ghost" size="sm" className="ml-auto" title="Hide this group until its projects change" onClick={() => dismiss.mutate(group.key)}>
          Not duplicates
        </Button>
      </header>

      <ul role="radiogroup" aria-label="Project to keep">
        {shown.map((p, i) => (
          <ProjectRow key={p.id} project={p} name={name} kept={p.id === keep.id} suggested={i === 0} onKeep={() => setKeepId(p.id)} />
        ))}
      </ul>
      {group.projects.length > PREVIEW_ROWS ? (
        <button
          type="button"
          className="w-full border-t px-4.5 py-2 text-left font-mono text-xs text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
          onClick={() => setExpanded((e) => !e)}
        >
          {expanded ? "Show fewer" : `Show all ${group.projects.length}`}
        </button>
      ) : null}

      <footer className="flex flex-wrap items-center gap-3 border-t bg-secondary/30 px-4.5 py-3">
        <p className="min-w-0 flex-1 text-sm text-muted-foreground">
          Keep <span className="font-medium text-foreground">{keep.name}</span>, merge the others into it and remove {formatCount(others.length, "copy", "copies")}.
        </p>
        <Button onClick={() => setConfirming(true)}>
          <Check />
          Keep this one
        </Button>
      </footer>

      <ResolveDialog open={confirming} keep={keep} others={others} onClose={() => setConfirming(false)} />
    </section>
  );
}

function ProjectRow({ project: p, name, kept, suggested, onKeep }: { project: DuplicateProject; name: string; kept: boolean; suggested: boolean; onKeep: () => void }) {
  const cover = p.main_image_id ?? p.preview_id;
  const meta = [
    formatCount(p.asset_count, "file"),
    formatBytes(p.total_bytes),
    `added ${timeAgo(p.created_at)}`,
    p.tags.length ? formatCount(p.tags.length, "tag") : "",
    p.collections.length ? formatCount(p.collections.length, "collection") : "",
  ].filter(Boolean);
  return (
    <li className={cn("border-b last:border-b-0", kept && "bg-primary/5")}>
      <label className="flex cursor-pointer items-center gap-3 px-4.5 py-2.5">
        <input type="radio" name={name} checked={kept} onChange={onKeep} className="size-4 shrink-0 accent-primary" />
        <div className="grid h-12 w-16 shrink-0 place-items-center overflow-hidden rounded-md border bg-thumb">
          {cover ? <img src={thumbUrl(p.id, cover)} alt="" loading="lazy" className="size-full object-cover" /> : <Box className="size-5 text-dim" />}
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <Link to={`/projects/${p.id}`} className="truncate font-medium hover:underline" onClick={(e) => e.stopPropagation()}>
              {p.name}
            </Link>
            {p.is_favourite ? <Star className="size-3.5 shrink-0 fill-current text-amber-500" aria-label="Favourite" /> : null}
            {suggested ? <span className="shrink-0 rounded-full border px-1.5 font-mono text-[10px] text-muted-foreground">suggested</span> : null}
          </div>
          <div className="truncate font-mono text-xs text-muted-foreground">{p.folder_path}</div>
          <div className="truncate font-mono text-[11px] text-dim">{meta.join(" · ")}</div>
        </div>
        {kept ? <span className="hidden font-mono text-xs text-primary sm:inline">keep</span> : null}
      </label>
    </li>
  );
}

/** What merging the others into `keep` adds: new tags, new collections, favourite, filled-in fields. */
function mergePreview(keep: DuplicateProject, others: DuplicateProject[]): string[] {
  const tagNames = new Set(keep.tags.map((t) => t.name.toLowerCase()));
  const tags = [...new Set(others.flatMap((o) => o.tags.map((t) => t.name)).filter((n) => !tagNames.has(n.toLowerCase())))];
  const collectionIds = new Set(keep.collections.map((c) => c.id));
  const collections = [...new Map(others.flatMap((o) => o.collections).filter((c) => !collectionIds.has(c.id)).map((c) => [c.id, c.name])).values()];
  const list = (names: string[]) => (names.length > 4 ? `${names.slice(0, 4).join(", ")} +${names.length - 4}` : names.join(", "));
  const out: string[] = [];
  if (tags.length) out.push(`Tags: ${list(tags)}`);
  if (collections.length) out.push(`Collections: ${list(collections)}`);
  if (!keep.is_favourite && others.some((o) => o.is_favourite)) out.push("Favourite");
  if (!keep.description.trim() && others.some((o) => o.description.trim())) out.push("Description");
  if (!keep.source_url && others.some((o) => o.source_url)) out.push("Source link");
  if (!keep.main_image_id && others.some((o) => o.main_image_id)) out.push("Cover picture (when it has the same image)");
  return out;
}

function ResolveDialog({ open, keep, others, onClose }: { open: boolean; keep: DuplicateProject; others: DuplicateProject[]; onClose: () => void }) {
  const days = useTrash().data?.retention_days ?? 0;
  const resolve = useResolveDuplicates();
  const merged = mergePreview(keep, others);
  const run = (remove: RemoveMode) => {
    resolve.mutate({ keep, removeIds: others.map((o) => o.id), remove });
    onClose();
  };
  const choice = "group rounded-xl border p-3.5 text-left transition-colors focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none";
  const n = formatCount(others.length, "copy", "copies");

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Keep “{keep.name}”?</DialogTitle>
          <DialogDescription>
            {merged.length ? `Moves onto it from the ${n}:` : `The ${n} have nothing it doesn't already have.`}
          </DialogDescription>
        </DialogHeader>
        {merged.length ? (
          <ul className="space-y-1 rounded-xl border bg-secondary/30 p-3 text-sm">
            {merged.map((m) => (
              <li key={m} className="flex gap-2">
                <Check className="mt-0.5 size-3.5 shrink-0 text-primary" />
                <span className="min-w-0">{m}</span>
              </li>
            ))}
          </ul>
        ) : null}
        <p className="text-sm font-medium">Then, for the {n}:</p>
        <div className="grid gap-2.5">
          <button type="button" className={`${choice} border-destructive/40 hover:bg-destructive/10`} onClick={() => run("trash")}>
            <div className="flex items-center gap-2 font-medium text-destructive">
              <Trash2 className="size-4" />
              {days > 0 ? "Move to trash" : "Delete files"}
            </div>
            <p className="mt-1 text-sm text-muted-foreground">
              {days > 0 ? `Their folders go to LiMa’s trash. Restore them from Admin → Trash for ${days} days.` : "Their folders are deleted from disk. This can’t be undone."}
            </p>
          </button>
          <button type="button" className={`${choice} hover:border-line-strong hover:bg-secondary/60`} onClick={() => run("keep_files")}>
            <div className="flex items-center gap-2 font-medium">
              <EyeOff className="size-4 text-muted-foreground" />
              Remove from LiMa
            </div>
            <p className="mt-1 text-sm text-muted-foreground">Files stay on disk and sync stops picking the folders up.</p>
          </button>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function SharedFiles({ files }: { files: SharedFileGroup[] }) {
  return (
    <section aria-label="Shared files" className="overflow-hidden rounded-2xl border bg-card">
      <header className="border-b px-4.5 py-3">
        <div className="font-medium">Same file in different projects</div>
        <p className="text-sm text-muted-foreground">These projects aren't copies of each other, they just share a file. Shown for reference.</p>
      </header>
      <ul>
        {files.map((f) => (
          <SharedFileRow key={f.key} file={f} />
        ))}
      </ul>
    </section>
  );
}

function SharedFileRow({ file }: { file: SharedFileGroup }) {
  const [expanded, setExpanded] = React.useState(false);
  const dismiss = useDismissDuplicates();
  const fileName = file.copies[0]?.file_path.split("/").pop() ?? "";
  const shown = expanded ? file.copies : file.copies.slice(0, PREVIEW_ROWS);
  const Icon = file.kind === "image" ? ImageIcon : FileBox;
  return (
    <li className="border-b px-4.5 py-3 last:border-b-0">
      <div className="flex items-center gap-2">
        <Icon className="size-4 shrink-0 text-dim" />
        <div className="flex min-w-0 flex-1 flex-wrap items-baseline gap-x-2">
          <span className="min-w-0 truncate font-medium">{fileName}</span>
          <span className="font-mono text-xs text-muted-foreground">
            {formatBytes(file.size_bytes)} · in {formatCount(new Set(file.copies.map((c) => c.project_id)).size, "project")}
          </span>
        </div>
        <Button variant="ghost" size="sm" className="shrink-0" title="Hide this file" onClick={() => dismiss.mutate(file.key)}>
          Hide
        </Button>
      </div>
      <div className="mt-1.5 flex flex-wrap gap-1.5">
        {shown.map((c) => (
          <Link
            key={c.asset_id}
            to={`/projects/${c.project_id}`}
            title={c.file_path}
            className="max-w-full truncate rounded-full border px-2 py-0.5 text-xs text-muted-foreground hover:border-line-strong hover:text-foreground"
          >
            {c.project_name}
          </Link>
        ))}
        {file.copies.length > PREVIEW_ROWS ? (
          <button type="button" className="rounded-full px-2 py-0.5 font-mono text-xs text-dim hover:text-foreground" onClick={() => setExpanded((e) => !e)}>
            {expanded ? "fewer" : `+${file.copies.length - PREVIEW_ROWS} more`}
          </button>
        ) : null}
      </div>
    </li>
  );
}
