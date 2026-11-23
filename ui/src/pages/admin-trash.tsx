import * as React from "react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Loader2, RotateCcw, Trash2 } from "lucide-react";
import { toast } from "sonner";

import { apiNoBody } from "@/lib/api";
import { formatBytes, timeAgo } from "@/lib/format";
import { apiErrorMessage } from "@/lib/tags";
import { refreshAfterTrash, restoreFromTrash, useTrash, type TrashItem } from "@/lib/trash";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { PageBody, PageHero, MicroLabel } from "@/components/layout";
import { AdminNav } from "@/components/admin-nav";
import { adminIndex } from "@/lib/admin";

/** "in 12 days", "today", ... */
function untilLabel(iso: string): string {
  const days = Math.ceil((new Date(iso).getTime() - Date.now()) / 86_400_000);
  if (days <= 0) return "today";
  return days === 1 ? "in 1 day" : `in ${days} days`;
}

/** Whole days until `iso` is deleted for good (negative once it's due). */
function daysUntil(iso: string): number {
  return Math.ceil((new Date(iso).getTime() - Date.now()) / 86_400_000);
}

export function AdminTrashPage() {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const trashQ = useTrash();
  const items = trashQ.data?.items ?? [];
  const days = trashQ.data?.retention_days ?? 0;
  const [confirm, setConfirm] = React.useState<TrashItem | "all" | null>(null);
  const [restoring, setRestoring] = React.useState<string | null>(null);

  const purgeM = useMutation({
    mutationFn: (target: TrashItem | "all") => apiNoBody("DELETE", target === "all" ? "/trash" : `/trash/${target.id}`),
    onSuccess: (_, target) => {
      toast.success(target === "all" ? "Trash emptied" : `Deleted “${target.name}” for good`);
      void refreshAfterTrash(qc);
    },
    onError: (err) => toast.error("Couldn't delete", { description: apiErrorMessage(err) }),
  });

  const restore = async (item: TrashItem) => {
    setRestoring(item.id);
    const projectId = await restoreFromTrash(qc, item.id, item.file_path ? (item.file_path.split("/").pop() ?? item.name) : item.name);
    setRestoring(null);
    if (projectId) navigate(`/projects/${projectId}`);
  };

  const totalBytes = items.reduce((n, i) => n + i.size_bytes, 0);

  const purgeAt = (item: TrashItem) => new Date(item.expires_at).getTime();
  const goneSoon = items.filter((i) => daysUntil(i.expires_at) <= 7).sort((a, b) => purgeAt(a) - purgeAt(b));
  const later = items
    .filter((i) => daysUntil(i.expires_at) > 7)
    .sort((a, b) => new Date(b.trashed_at).getTime() - new Date(a.trashed_at).getTime());

  const renderRow = (item: TrashItem, goneSoon: boolean) => (
    <li key={item.id} className="flex flex-wrap items-center gap-3 border-b px-4.5 py-3 last:border-b-0">
      <div className="min-w-0 flex-1">
        <div className="truncate font-medium">{item.file_path ? item.file_path.split("/").pop() : item.name}</div>
        <div className="truncate font-mono text-xs text-muted-foreground">
          {item.file_path
            ? `file from ${item.name} · ${item.folder_path}/${item.file_path} · ${formatBytes(item.size_bytes)}`
            : `${item.folder_path} · ${item.file_count} file${item.file_count === 1 ? "" : "s"} · ${formatBytes(item.size_bytes)}`}
        </div>
        <div className="font-mono text-[11px] text-dim">deleted {timeAgo(item.trashed_at)}</div>
        {goneSoon ? (
          <div className="font-mono text-xs text-warn">gone {untilLabel(item.expires_at)}</div>
        ) : (
          <div className="font-mono text-[11px] text-dim">gone for good {untilLabel(item.expires_at)}</div>
        )}
      </div>
      <div className="flex gap-2">
        <Button variant="outline" size="sm" disabled={restoring !== null} onClick={() => void restore(item)}>
          {restoring === item.id ? <Loader2 className="animate-spin" /> : <RotateCcw />}
          Restore
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="text-destructive hover:text-destructive"
          disabled={purgeM.isPending}
          onClick={() => setConfirm(item)}
        >
          Delete forever
        </Button>
      </div>
    </li>
  );

  return (
    <div>
      <PageHero
        index={adminIndex("/admin/trash")}
        label="Trash"
        title="Trash"
        subtitle={
          days > 0
            ? `Deleted projects and files stay here for ${days} days · restoring a project brings back its tags, collections and description too`
            : "The trash is off (LIMA_TRASH_DAYS=0): deletes are final"
        }
      >
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" disabled={items.length === 0 || purgeM.isPending} onClick={() => setConfirm("all")}>
            <Trash2 />
            Empty trash
          </Button>
        </div>
      </PageHero>

      <PageBody className="flex flex-col gap-6 pt-7 pb-16">
        <AdminNav trashCount={items.length} />
        {trashQ.isError ? <div className="text-sm text-destructive">{apiErrorMessage(trashQ.error)}</div> : null}
        {trashQ.isSuccess && items.length === 0 ? (
          <div className="bg-dots rounded-2xl border border-dashed p-10 text-center">
            <div className="font-display text-xl font-semibold">The trash is empty</div>
            <p className="mt-2 text-sm text-muted-foreground">Deleted projects and files show up here so you can bring them back.</p>
          </div>
        ) : null}

        {items.length > 0 ? (
          <>
            <div className="font-mono text-xs text-muted-foreground">
              {items.length} item{items.length === 1 ? "" : "s"} · {formatBytes(totalBytes)}
            </div>
            {goneSoon.length > 0 ? (
              <section aria-label="Gone soon" className="overflow-hidden rounded-2xl border bg-card">
                <header className="flex flex-wrap items-baseline gap-x-2 gap-y-1 border-b px-4.5 py-3">
                  <MicroLabel className="text-warn">Gone soon</MicroLabel>
                  <span className="font-mono text-[11px] text-dim">deleted for good within 7 days</span>
                </header>
                <ul>{goneSoon.map((item) => renderRow(item, true))}</ul>
              </section>
            ) : null}
            {later.length > 0 ? (
              <section aria-label="Later" className="overflow-hidden rounded-2xl border bg-card">
                <header className="border-b px-4.5 py-3">
                  <MicroLabel>Later</MicroLabel>
                </header>
                <ul>{later.map((item) => renderRow(item, false))}</ul>
              </section>
            ) : null}
          </>
        ) : null}
      </PageBody>

      <Dialog open={confirm !== null} onOpenChange={(o) => !o && setConfirm(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{confirm === "all" ? "Empty the trash?" : `Delete “${confirm?.name}” for good?`}</DialogTitle>
            <DialogDescription>
              {confirm === "all"
                ? `${items.length} item${items.length === 1 ? "" : "s"} (${formatBytes(totalBytes)}) will be deleted from disk. This can’t be undone.`
                : "Its folder will be deleted from disk. This can’t be undone."}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirm(null)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                if (confirm) purgeM.mutate(confirm);
                setConfirm(null);
              }}
            >
              Delete forever
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
