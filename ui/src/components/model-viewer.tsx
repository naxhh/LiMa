import * as React from "react";
import { Box, Loader2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { formatBytes } from "@/lib/format";
import { ModelPoster } from "@/components/model-poster";
import type { ViewerFormat } from "@/lib/viewer";

// three.js only loads when a model is actually shown.
const ModelViewerCanvas = React.lazy(() => import("./model-viewer-canvas"));

/** Bigger files ask first: they can take a while and use a lot of memory. */
const AUTOLOAD_BYTES = 50 * 1024 * 1024;

/**
 * 3MF is zipped XML: a lithophane of 13 MB unpacks to a 117 MB model part, and three.js parses
 * that on the main thread. Ask well before the generic limit or the page freezes.
 */
const AUTOLOAD_3MF_BYTES = 5 * 1024 * 1024;

function autoloads(format: ViewerFormat, sizeBytes: number): boolean {
  return sizeBytes <= (format === "3mf" ? AUTOLOAD_3MF_BYTES : AUTOLOAD_BYTES);
}

/** Interactive 3D preview that fills its (positioned) parent. */
export function ModelViewer({
  url,
  format,
  sizeBytes,
  posterUrl,
  previewUrl,
}: {
  url: string;
  format: ViewerFormat;
  sizeBytes: number;
  /** Server-rendered preview, shown while waiting and behind the loading canvas. */
  posterUrl?: string;
  /** Server-decimated mesh: cheap enough to load without asking, whatever the file size. */
  previewUrl?: string;
}) {
  const [confirmed, setConfirmed] = React.useState(!!previewUrl || autoloads(format, sizeBytes));

  // A different file starts unconfirmed again if it's big.
  const [lastUrl, setLastUrl] = React.useState(url);
  if (url !== lastUrl) {
    setLastUrl(url);
    setConfirmed(!!previewUrl || autoloads(format, sizeBytes));
  }

  if (!confirmed) {
    return (
      <div className="absolute inset-0">
        <ModelPoster url={posterUrl} />
        <div className="absolute inset-0 grid place-items-center bg-background/50 p-6 text-center backdrop-blur-[1px]">
          <div className="max-w-xs space-y-3">
            <Box className="mx-auto size-7 text-dim" />
            <p className="text-sm text-muted-foreground">
              This model is <span className="font-mono text-foreground">{formatBytes(sizeBytes)}</span>. Previewing it can be slow and use a
              lot of memory.
            </p>
            <Button variant="outline" onClick={() => setConfirmed(true)}>
              Load 3D preview
            </Button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <React.Suspense
      fallback={
        <>
          <ModelPoster url={posterUrl} />
          <div className="absolute inset-0 grid place-items-center">
            <div className="flex items-center gap-2 font-mono text-xs text-muted-foreground">
              <Loader2 className="size-3.5 animate-spin" />
              loading viewer
            </div>
          </div>
        </>
      }
    >
      <ModelViewerCanvas key={url} url={url} format={format} previewUrl={previewUrl} posterUrl={posterUrl} />
    </React.Suspense>
  );
}
