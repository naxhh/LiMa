import * as React from "react";

import { cn } from "@/lib/utils";

/** Server-rendered preview image, dimmed and shown until the 3D viewer has a model to draw. */
export function ModelPoster({ url, className }: { url?: string; className?: string }) {
  const [failed, setFailed] = React.useState(false);
  React.useEffect(() => setFailed(false), [url]);

  if (!url || failed) return null;
  return (
    <img
      src={url}
      alt=""
      aria-hidden
      onError={() => setFailed(true)}
      className={cn("pointer-events-none absolute inset-0 h-full w-full object-contain opacity-45", className)}
    />
  );
}
