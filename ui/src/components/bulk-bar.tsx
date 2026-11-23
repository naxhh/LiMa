import * as React from "react";
import { CheckCheck, Download, EyeOff, Layers, Star, Tag as TagIcon, Trash2, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { downloadProjects, useBulkAction, type BulkAction } from "@/lib/bulk";
import { useCollections } from "@/lib/collections";
import type { Selection } from "@/lib/selection";
import { useAllTags } from "@/lib/tags";
import { useTrash } from "@/lib/trash";

const plural = (n: number) => `${n} project${n === 1 ? "" : "s"}`;

/** Floating bar with actions for the selected projects. */
/**
 * `total` is how many projects match (null when unknown); `onSelectAll` selects all of them,
 * loaded or not (defaults to the loaded cards).
 */
export function BulkBar({ selection, total, onSelectAll }: { selection: Selection; total: number | null; onSelectAll?: () => void }) {
  const [dialog, setDialog] = React.useState<"tags" | "collections" | "delete" | null>(null);
  const bulk = useBulkAction(selection.exit);
  const n = selection.ids.length;
  const run = (action: BulkAction) => {
    bulk.mutate({ ...action, ids: selection.ids } as Parameters<typeof bulk.mutate>[0]);
    setDialog(null);
  };
  if (!selection.active) return null;

  const action = "gap-1.5 [&_svg]:size-4";
  return (
    <>
      <div
        role="toolbar"
        aria-label="Selected projects"
        className="fixed inset-x-4 bottom-[88px] z-40 mx-auto flex max-w-3xl flex-wrap items-center gap-1 rounded-2xl sm:gap-1.5 border border-line-strong bg-card/95 p-2 shadow-2xl backdrop-blur sm:bottom-6"
      >
        <div className="flex items-center gap-2 px-2">
          <span className="font-mono text-sm tabular-nums">{n}</span>
          <span className="hidden text-sm text-muted-foreground sm:inline">selected</span>
        </div>
        <Button
          variant="ghost"
          size="sm"
          className={action}
          aria-label={n > 0 && n === total ? "Select none" : `Select all${total ? ` ${total}` : ""}`}
          onClick={n > 0 && n === total ? selection.clear : (onSelectAll ?? selection.selectAll)}
        >
          <CheckCheck />
          <span className="hidden sm:inline">{n > 0 && n === total ? "None" : `All${total ? ` ${total}` : ""}`}</span>
        </Button>
        <div className="mx-1 hidden h-6 w-px bg-border sm:block" />
        <Button variant="ghost" size="sm" className={action} disabled={!n} aria-label="Tags" onClick={() => setDialog("tags")}>
          <TagIcon />
          <span className="hidden sm:inline">Tags</span>
        </Button>
        <Button variant="ghost" size="sm" className={action} disabled={!n} aria-label="Collection" onClick={() => setDialog("collections")}>
          <Layers />
          <span className="hidden sm:inline">Collection</span>
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="sm" className={action} disabled={!n} aria-label="Favourite">
              <Star />
              <span className="hidden sm:inline">Favourite</span>
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="center">
            <DropdownMenuItem onClick={() => run({ action: "favourite" })}>Add to favourites</DropdownMenuItem>
            <DropdownMenuItem onClick={() => run({ action: "unfavourite" })}>Remove from favourites</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
        <Button variant="ghost" size="sm" className={action} disabled={!n} aria-label="Download" onClick={() => downloadProjects(selection.ids)}>
          <Download />
          <span className="hidden sm:inline">Download</span>
        </Button>
        <Button variant="ghost" size="sm" className={`${action} text-destructive hover:text-destructive`} disabled={!n} aria-label="Delete" onClick={() => setDialog("delete")}>
          <Trash2 />
          <span className="hidden sm:inline">Delete</span>
        </Button>
        <Button variant="ghost" size="icon-sm" className="ml-auto" aria-label="Done selecting" title="Done (Esc)" onClick={selection.exit}>
          <X />
        </Button>
      </div>

      <BulkTagsDialog open={dialog === "tags"} count={n} onClose={() => setDialog(null)} onRun={run} />
      <BulkCollectionsDialog open={dialog === "collections"} count={n} onClose={() => setDialog(null)} onRun={run} />
      <BulkDeleteDialog open={dialog === "delete"} count={n} onClose={() => setDialog(null)} onRun={run} />
    </>
  );
}

type DialogProps = { open: boolean; count: number; onClose: () => void; onRun: (a: BulkAction) => void };

function BulkTagsDialog({ open, count, onClose, onRun }: DialogProps) {
  const [text, setText] = React.useState("");
  const tags = useAllTags().data ?? [];
  React.useEffect(() => {
    if (open) setText("");
  }, [open]);
  const names = text.split(",").map((t) => t.trim()).filter(Boolean);
  const typing = text.split(",").pop()?.trim().toLowerCase() ?? "";
  const suggestions = tags.filter((t) => !names.some((n) => n.toLowerCase() === t.name.toLowerCase()) && t.name.toLowerCase().includes(typing)).slice(0, 8);
  const pick = (name: string) => {
    const parts = text.split(",");
    parts[parts.length - 1] = ` ${name}`;
    setText(parts.join(",").replace(/^ /, "") + ", ");
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Tags for {plural(count)}</DialogTitle>
          <DialogDescription>Comma-separated. New tags are created when added.</DialogDescription>
        </DialogHeader>
        <Input autoFocus value={text} onChange={(e) => setText(e.target.value)} placeholder="PLA, gift, desk" maxLength={500} />
        {suggestions.length > 0 ? (
          <div className="flex flex-wrap gap-1.5">
            {suggestions.map((t) => (
              <button
                key={t.id}
                type="button"
                onClick={() => pick(t.name)}
                className="rounded-full border px-2 py-0.5 text-xs text-muted-foreground hover:border-line-strong hover:text-foreground"
              >
                {t.name}
              </button>
            ))}
          </div>
        ) : null}
        <DialogFooter>
          <Button variant="outline" disabled={!names.length} onClick={() => onRun({ action: "remove_tags", tags: names })}>
            Remove
          </Button>
          <Button disabled={!names.length} onClick={() => onRun({ action: "add_tags", tags: names })}>
            Add
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function BulkCollectionsDialog({ open, count, onClose, onRun }: DialogProps) {
  const collections = (useCollections().data ?? []).filter((c) => !c.is_favourites);
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Collections for {plural(count)}</DialogTitle>
          <DialogDescription>Add all of them to a collection, or take them out of one.</DialogDescription>
        </DialogHeader>
        {collections.length === 0 ? (
          <p className="text-sm text-muted-foreground">No collections yet. Create one from the Collections page.</p>
        ) : (
          <ul className="-mx-2 max-h-80 overflow-y-auto">
            {collections.map((c) => (
              <li key={c.id} className="flex items-center gap-2 rounded-lg px-2 py-1.5 hover:bg-secondary/60">
                <Layers className="size-4 shrink-0 text-dim" />
                <span className="min-w-0 flex-1 truncate">{c.name}</span>
                <span className="font-mono text-xs text-dim">{c.project_count}</span>
                <Button variant="ghost" size="sm" onClick={() => onRun({ action: "remove_from_collection", collection_id: c.id })}>
                  Remove
                </Button>
                <Button variant="outline" size="sm" onClick={() => onRun({ action: "add_to_collection", collection_id: c.id })}>
                  Add
                </Button>
              </li>
            ))}
          </ul>
        )}
      </DialogContent>
    </Dialog>
  );
}

function BulkDeleteDialog({ open, count, onClose, onRun }: DialogProps) {
  const days = useTrash().data?.retention_days ?? 0;
  const choice = "group rounded-xl border p-3.5 text-left transition-colors focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none";
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Remove {plural(count)}?</DialogTitle>
          <DialogDescription>Choose what happens to their folders.</DialogDescription>
        </DialogHeader>
        <div className="grid gap-2.5">
          <button type="button" className={`${choice} hover:border-line-strong hover:bg-secondary/60`} onClick={() => onRun({ action: "remove_keep_files" })}>
            <div className="flex items-center gap-2 font-medium">
              <EyeOff className="size-4 text-muted-foreground" />
              Remove from LiMa
            </div>
            <p className="mt-1 text-sm text-muted-foreground">Files stay on disk and sync stops picking the folders up. Tags, collections and descriptions are dropped.</p>
          </button>
          <button type="button" className={`${choice} border-destructive/40 hover:bg-destructive/10`} onClick={() => onRun({ action: "delete" })}>
            <div className="flex items-center gap-2 font-medium text-destructive">
              <Trash2 className="size-4" />
              {days > 0 ? "Move to trash" : "Delete files"}
            </div>
            <p className="mt-1 text-sm text-muted-foreground">
              {days > 0
                ? `The folders go to LiMa’s trash. Restore them, with tags and collections, from Admin → Trash for ${days} days.`
                : "The folders are deleted from disk. This can’t be undone."}
            </p>
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
