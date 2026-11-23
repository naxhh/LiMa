import * as React from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { apiJson } from "@/lib/api";
import { PROJECT_CACHE_KEYS, optimisticHandlers, patchProjects } from "@/lib/optimistic";
import { TAGS_QUERY_KEY, apiErrorMessage, type Tag } from "@/lib/tags";
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

type TagChange = { name?: string; color?: string };

/** Renames / recolors a tag everywhere it's shown, right away. */
function useEditTag() {
  const qc = useQueryClient();
  // The tag travels with the call: the dialog forgets it as soon as it closes.
  type Vars = { tag: Tag; body: TagChange };
  return useMutation({
    mutationFn: ({ tag, body }: Vars) => apiJson("PATCH", `/tags/${tag.id}`, body),
    ...optimisticHandlers<Vars>(qc, {
      mutationKey: ["tag-edit"],
      keys: [...PROJECT_CACHE_KEYS, TAGS_QUERY_KEY],
      apply: ({ tag, body }) => {
        const change = <T extends { id: string; name: string; color: string }>(x: T): T =>
          x.id === tag.id ? { ...x, ...(body.name ? { name: body.name } : {}), ...(body.color ? { color: body.color } : {}) } : x;
        qc.setQueryData<Tag[]>(TAGS_QUERY_KEY, (list) => list?.map(change));
        patchProjects(qc, (p) => (p.tags.some((x) => x.id === tag.id) ? { ...p, tags: p.tags.map(change) } : p));
      },
      errorTitle: ({ tag }) => `Couldn't update “${tag.name}”`,
      invalidate: [...PROJECT_CACHE_KEYS, TAGS_QUERY_KEY],
    }),
  });
}

/** Create (no `tag`) or edit dialog. */
export function TagFormDialog({
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

  // Creating waits for the server (name clashes show inline); editing applies right away.
  const createM = useMutation({
    mutationFn: (body: TagChange) => apiJson("POST", "/tags", body),
    onSuccess: async (_, body) => {
      await qc.invalidateQueries({ queryKey: TAGS_QUERY_KEY });
      toast.success(`Created tag “${body.name ?? ""}”`);
      onOpenChange(false);
    },
  });
  const editM = useEditTag();

  const submit = () => {
    const body: TagChange = {};
    const clean = name.trim();
    if (!tag || clean !== tag.name) body.name = clean;
    if (color && color !== tag?.color) body.color = color;
    if (!tag) return createM.mutate(body);
    if (Object.keys(body).length > 0) editM.mutate({ tag, body });
    onOpenChange(false);
  };

  React.useEffect(() => {
    if (open) createM.reset();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const canSave = name.trim().length > 0 && !createM.isPending;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <form
          className="space-y-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (canSave) submit();
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

          {createM.isError ? <div className="text-sm text-destructive">{apiErrorMessage(createM.error)}</div> : null}

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
