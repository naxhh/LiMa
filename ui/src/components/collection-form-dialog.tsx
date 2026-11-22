import * as React from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { apiJson } from "@/lib/api";
import { invalidateCollections, type Collection } from "@/lib/collections";
import { apiErrorMessage } from "@/lib/tags";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";

/** Create (no `collection`) or edit dialog. `onSaved` gets the saved collection. */
export function CollectionFormDialog({
  collection,
  open,
  onOpenChange,
  onSaved,
}: {
  collection?: Pick<Collection, "id" | "name" | "description" | "is_favourites">;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved?: (c: Collection) => void;
}) {
  const qc = useQueryClient();
  const [name, setName] = React.useState("");
  const [description, setDescription] = React.useState("");

  React.useEffect(() => {
    if (!open) return;
    setName(collection?.name ?? "");
    setDescription(collection?.description ?? "");
  }, [open, collection]);

  const saveM = useMutation({
    mutationFn: () => {
      const body = { name: name.trim(), description: description.trim() };
      return collection
        ? apiJson<Collection>("PATCH", `/collections/${collection.id}`, body)
        : apiJson<Collection>("POST", "/collections", body);
    },
    onSuccess: async (saved) => {
      await invalidateCollections(qc);
      onOpenChange(false);
      onSaved?.(saved);
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
            <DialogTitle>{collection ? "Edit collection" : "New collection"}</DialogTitle>
            <DialogDescription>A hand-picked group of projects. Projects can be in several collections.</DialogDescription>
          </DialogHeader>
          <div className="space-y-1.5">
            <label htmlFor="collection-name" className="text-sm font-medium">
              Name
            </label>
            <Input
              id="collection-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              maxLength={100}
              placeholder="Desk toys, Gifts 2026, Printer upgrades…"
              disabled={collection?.is_favourites}
              autoFocus={!collection?.is_favourites}
            />
            {collection?.is_favourites ? (
              <p className="text-xs text-muted-foreground">Favourites is built in, so it can’t be renamed.</p>
            ) : null}
          </div>
          <div className="space-y-1.5">
            <label htmlFor="collection-description" className="text-sm font-medium">
              Description <span className="font-normal text-muted-foreground">(optional)</span>
            </label>
            <Textarea
              id="collection-description"
              rows={3}
              maxLength={2000}
              value={description}
              onChange={(e) => setDescription(e.target.value)}
            />
          </div>
          {saveM.isError ? <div className="text-sm text-destructive">{apiErrorMessage(saveM.error)}</div> : null}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={!canSave}>
              {collection ? "Save" : "Create collection"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
