import * as React from "react";
import { Link } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Pencil, Plus, Trash2 } from "lucide-react";

import { apiJson, apiNoBody } from "@/lib/api";
import { TAGS_QUERY_KEY, apiErrorMessage, useAllTags, type Tag } from "@/lib/tags";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { TagBadge } from "@/components/tag-badge";
import { TagColorPicker } from "@/components/tag-color-picker";

export function TagsPage() {
  const tagsQ = useAllTags();
  const [filter, setFilter] = React.useState("");
  const [creating, setCreating] = React.useState(false);
  const [editing, setEditing] = React.useState<Tag | null>(null);
  const [deleting, setDeleting] = React.useState<Tag | null>(null);

  const tags = tagsQ.data ?? [];
  const needle = filter.trim().toLowerCase();
  const shown = needle ? tags.filter((t) => t.name.toLowerCase().includes(needle)) : tags;

  return (
    <div className="p-6 space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold">Tags</h1>
          <p className="text-sm text-muted-foreground">
            {tags.length} tag{tags.length === 1 ? "" : "s"}. Renaming or deleting a tag updates every project using it.
          </p>
        </div>
        <Button onClick={() => setCreating(true)}>
          <Plus />
          New tag
        </Button>
      </div>

      {tags.length > 8 ? (
        <Input value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="Filter tags…" />
      ) : null}

      {tagsQ.isLoading ? <div className="text-sm">Loading…</div> : null}
      {tagsQ.isError ? (
        <div className="text-sm text-destructive">Failed to load tags: {apiErrorMessage(tagsQ.error)}</div>
      ) : null}
      {!tagsQ.isLoading && tags.length === 0 ? (
        <div className="rounded-lg border border-dashed p-6 text-sm text-muted-foreground">
          No tags yet. Create one here or add tags from a project page.
        </div>
      ) : null}

      {shown.length > 0 ? (
        <ul className="divide-y rounded-lg border">
          {shown.map((t) => (
            <li key={t.id} className="flex items-center gap-3 px-4 py-2.5">
              <TagBadge name={t.name} color={t.color} className="min-w-0" />
              <Link
                to={`/?query=${encodeURIComponent(t.name)}`}
                className="text-sm text-muted-foreground underline-offset-4 hover:underline"
              >
                {t.project_count} project{t.project_count === 1 ? "" : "s"}
              </Link>
              <div className="ml-auto flex gap-1">
                <Button variant="ghost" size="icon-sm" onClick={() => setEditing(t)} aria-label={`Edit ${t.name}`}>
                  <Pencil />
                </Button>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  onClick={() => setDeleting(t)}
                  aria-label={`Delete ${t.name}`}
                  className="text-destructive hover:text-destructive"
                >
                  <Trash2 />
                </Button>
              </div>
            </li>
          ))}
        </ul>
      ) : null}
      {needle && shown.length === 0 && tags.length > 0 ? (
        <div className="text-sm text-muted-foreground">No tags match “{filter}”.</div>
      ) : null}

      <TagFormDialog open={creating} onOpenChange={setCreating} />
      <TagFormDialog
        tag={editing ?? undefined}
        open={editing !== null}
        onOpenChange={(o) => !o && setEditing(null)}
      />
      <DeleteTagDialog tag={deleting} onOpenChange={(o) => !o && setDeleting(null)} />
    </div>
  );
}

/** Create (no `tag`) or edit dialog. */
function TagFormDialog({
  tag,
  open,
  onOpenChange,
}: {
  tag?: Tag;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const qc = useQueryClient();
  const [name, setName] = React.useState("");
  const [color, setColor] = React.useState<string | null>(null);

  React.useEffect(() => {
    if (!open) return;
    setName(tag?.name ?? "");
    setColor(tag?.color ?? null);
  }, [open, tag]);

  const saveM = useMutation({
    mutationFn: () => {
      const body: { name?: string; color?: string } = {};
      const clean = name.trim();
      if (!tag || clean !== tag.name) body.name = clean;
      if (color && color !== tag?.color) body.color = color;
      if (tag && Object.keys(body).length === 0) return Promise.resolve();
      return tag ? apiJson("PATCH", `/tags/${tag.id}`, body) : apiJson("POST", "/tags", body);
    },
    onSuccess: async () => {
      await Promise.all([
        qc.invalidateQueries({ queryKey: TAGS_QUERY_KEY }),
        // Projects show tag names and colors.
        qc.invalidateQueries({ queryKey: ["project"] }),
        qc.invalidateQueries({ queryKey: ["projects"] }),
      ]);
      onOpenChange(false);
    },
  });

  React.useEffect(() => {
    if (open) saveM.reset();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const canSave = name.trim().length > 0 && !saveM.isPending;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <form
          className="space-y-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (canSave) saveM.mutate();
          }}
        >
          <DialogHeader>
            <DialogTitle>{tag ? "Edit tag" : "New tag"}</DialogTitle>
            {tag && tag.project_count > 0 ? (
              <DialogDescription>
                Used by {tag.project_count} project{tag.project_count === 1 ? "" : "s"}; they all get the change.
              </DialogDescription>
            ) : null}
          </DialogHeader>

          <div className="space-y-1">
            <label className="text-sm font-medium" htmlFor="tag-name">
              Name
            </label>
            <Input id="tag-name" value={name} onChange={(e) => setName(e.target.value)} maxLength={64} autoFocus />
          </div>

          <div className="space-y-2">
            <div className="text-sm font-medium">Color</div>
            <TagColorPicker value={color} onChange={setColor} allowAuto={!tag} />
          </div>

          <div className="space-y-1">
            <div className="text-xs text-muted-foreground">Preview</div>
            <TagBadge name={name.trim() || "tag"} color={color ?? tag?.color ?? "#64748B"} />
          </div>

          {saveM.isError ? <div className="text-sm text-destructive">{apiErrorMessage(saveM.error)}</div> : null}

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={!canSave}>
              {tag ? "Save" : "Create"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function DeleteTagDialog({ tag, onOpenChange }: { tag: Tag | null; onOpenChange: (open: boolean) => void }) {
  const qc = useQueryClient();
  const deleteM = useMutation({
    mutationFn: (id: string) => apiNoBody("DELETE", `/tags/${id}`),
    onSuccess: async () => {
      await Promise.all([
        qc.invalidateQueries({ queryKey: TAGS_QUERY_KEY }),
        qc.invalidateQueries({ queryKey: ["project"] }),
        qc.invalidateQueries({ queryKey: ["projects"] }),
      ]);
      onOpenChange(false);
    },
  });

  React.useEffect(() => {
    if (tag) deleteM.reset();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tag]);

  return (
    <Dialog open={tag !== null} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Delete tag “{tag?.name}”?</DialogTitle>
          <DialogDescription>
            {tag && tag.project_count > 0
              ? `It will be removed from ${tag.project_count} project${tag.project_count === 1 ? "" : "s"}. Projects and files are not affected.`
              : "No projects use this tag."}
          </DialogDescription>
        </DialogHeader>
        {deleteM.isError ? <div className="text-sm text-destructive">{apiErrorMessage(deleteM.error)}</div> : null}
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button variant="destructive" disabled={!tag || deleteM.isPending} onClick={() => tag && deleteM.mutate(tag.id)}>
            Delete
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
