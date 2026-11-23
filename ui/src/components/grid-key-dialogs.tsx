import { BulkCollectionsDialog, BulkDeleteDialog, BulkTagsDialog } from "@/components/bulk-bar";
import { CollectionPickerDialog } from "@/components/collection-picker-dialog";
import { TagPickerDialog } from "@/components/tag-picker-dialog";
import type { GridKeys } from "@/lib/grid-keys";

/** The dialogs a grid's `t` / `a` / `#` keys open: bulk ones for a selection, the card's own pickers for one project. */
export function GridKeyDialogs({ grid }: { grid: GridKeys }) {
  const { dialog, dialogProject: p, closeDialog, run } = grid;
  if (!dialog) return null;
  const onOpenChange = (open: boolean) => !open && closeDialog();

  if (dialog.scope === "selection") {
    const props = { open: true, count: grid.selectionCount, onClose: closeDialog, onRun: run };
    if (dialog.kind === "tags") return <BulkTagsDialog {...props} />;
    if (dialog.kind === "collections") return <BulkCollectionsDialog {...props} />;
    return <BulkDeleteDialog {...props} />;
  }
  if (!p) return null;
  if (dialog.kind === "tags") return <TagPickerDialog projectId={p.id} tags={p.tags} open onOpenChange={onOpenChange} />;
  if (dialog.kind === "collections") {
    return <CollectionPickerDialog projectId={p.id} collections={p.collections} isFavourite={p.is_favourite} open onOpenChange={onOpenChange} />;
  }
  return <BulkDeleteDialog open count={1} onClose={closeDialog} onRun={run} />;
}
