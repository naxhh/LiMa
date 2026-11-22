import * as React from "react";
import { useQuery } from "@tanstack/react-query";
import { Box, Download, File as FileIcon, Image as ImageIcon, Loader2, Lock, Rotate3d } from "lucide-react";

import type { components } from "@/gen/openapi";
import { apiGet } from "@/lib/api";
import { formatBytes } from "@/lib/format";
import { zipEntryUrl } from "@/lib/media";
import { apiErrorMessage } from "@/lib/tags";
import { viewerFormat } from "@/lib/viewer";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { ModelViewer } from "@/components/model-viewer";

type ZipContents = components["schemas"]["ZipContentsResponse"];
type ZipEntry = components["schemas"]["ZipEntry"];

/** Rows shown before "show all". */
const PREVIEW_ROWS = 50;

/** What's inside a zip asset, read in place: nothing is extracted. */
export function ZipContentsPanel({ projectId, assetId }: { projectId: string; assetId: string }) {
  const q = useQuery({
    queryKey: ["zip", projectId, assetId],
    queryFn: () => apiGet<ZipContents>(`/projects/${encodeURIComponent(projectId)}/assets/${encodeURIComponent(assetId)}/zip`),
    staleTime: 60_000,
  });
  const [all, setAll] = React.useState(false);
  const [viewing, setViewing] = React.useState<ZipEntry | null>(null);

  if (q.isPending) {
    return (
      <div className="flex items-center gap-2 px-4 py-3 font-mono text-xs text-muted-foreground">
        <Loader2 className="size-3.5 animate-spin" />
        reading archive
      </div>
    );
  }
  if (q.isError) return <div className="px-4 py-3 text-sm text-destructive">{apiErrorMessage(q.error)}</div>;

  const { entries, total_size, truncated } = q.data;
  const shown = all ? entries : entries.slice(0, PREVIEW_ROWS);
  const models = entries.filter((e) => e.kind === "model").length;

  return (
    <div className="bg-secondary/30">
      <div className="px-4 pt-2.5 pb-1.5 font-mono text-[11px] text-muted-foreground">
        {entries.length}
        {truncated ? "+" : ""} file{entries.length === 1 ? "" : "s"} · {models} model{models === 1 ? "" : "s"} · {formatBytes(total_size)} unpacked
      </div>
      {entries.length === 0 ? <div className="px-4 pb-3 text-sm text-muted-foreground">The archive is empty.</div> : null}
      <ul>
        {shown.map((e) => {
          const Icon = e.kind === "model" ? Box : e.kind === "image" ? ImageIcon : FileIcon;
          const viewable = e.supported && e.kind === "model" && viewerFormat(e.path);
          return (
            <li key={e.path} className="flex items-center gap-2.5 py-1 pr-3 pl-6 hover:bg-secondary/60">
              <Icon className={cn("size-3.5 shrink-0", e.kind === "model" ? "text-signal" : "text-dim")} />
              <span className="min-w-0 flex-1 truncate font-mono text-xs" title={e.path}>
                {e.path}
              </span>
              {!e.supported ? (
                <span
                  className="flex shrink-0 items-center gap-1 font-mono text-[10px] text-dim"
                  title={e.encrypted ? "Password protected" : "Unsupported compression"}
                >
                  <Lock className="size-3" />
                  {e.encrypted ? "locked" : "unsupported"}
                </span>
              ) : null}
              <span className="shrink-0 font-mono text-[11px] text-muted-foreground">{formatBytes(e.size)}</span>
              <div className="flex shrink-0 gap-1">
                {viewable ? (
                  <Button variant="ghost" size="icon-sm" onClick={() => setViewing(e)} aria-label={`View ${e.path} from the zip in 3D`} title="View in 3D">
                    <Rotate3d />
                  </Button>
                ) : null}
                {e.supported ? (
                  <Button variant="ghost" size="icon-sm" asChild>
                    <a href={zipEntryUrl(projectId, assetId, e.path, true)} aria-label={`Download ${e.path} from the zip`} title="Download">
                      <Download />
                    </a>
                  </Button>
                ) : null}
              </div>
            </li>
          );
        })}
      </ul>
      {entries.length > PREVIEW_ROWS ? (
        <button
          type="button"
          className="w-full px-6 py-2 text-left font-mono text-xs text-muted-foreground hover:text-foreground"
          onClick={() => setAll((v) => !v)}
        >
          {all ? "Show fewer" : `Show all ${entries.length}`}
        </button>
      ) : (
        <div className="h-1.5" />
      )}

      <Dialog open={viewing !== null} onOpenChange={(o) => !o && setViewing(null)}>
        <DialogContent className="sm:max-w-3xl">
          <DialogHeader>
            <DialogTitle className="truncate">{viewing?.path.split("/").pop()}</DialogTitle>
            <DialogDescription className="truncate font-mono text-xs">{viewing?.path}</DialogDescription>
          </DialogHeader>
          {viewing ? (
            <div className="relative aspect-[4/3] overflow-hidden rounded-xl border bg-thumb">
              <ModelViewer url={zipEntryUrl(projectId, assetId, viewing.path)} format={viewerFormat(viewing.path)!} sizeBytes={viewing.size} />
            </div>
          ) : null}
        </DialogContent>
      </Dialog>
    </div>
  );
}
