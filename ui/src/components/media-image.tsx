import * as React from "react";

import { cn } from "@/lib/utils";
import { ModelPlaceholder } from "@/components/model-placeholder";

/** Cover image filling its (relative) parent; falls back to the wireframe placeholder. */
export function MediaImage(props: { src: string; alt: string; className?: string; fallbackLabel?: string }) {
  const { src, alt, className, fallbackLabel } = props;
  const [failed, setFailed] = React.useState(false);

  React.useEffect(() => setFailed(false), [src]);

  if (failed) return <ModelPlaceholder label={fallbackLabel ?? "no preview"} />;

  return (
    <img
      src={src}
      alt={alt}
      className={cn("absolute inset-0 h-full w-full object-cover", className)}
      loading="lazy"
      onError={() => setFailed(true)}
    />
  );
}
