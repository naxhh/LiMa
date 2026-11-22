import * as React from "react";
import { X } from "lucide-react";

import { tagTextColor } from "@/lib/tags";
import { cn } from "@/lib/utils";

/** Colored tag pill. Pass `onRemove` to show a remove button. */
export function TagBadge({
  name,
  color,
  onRemove,
  className,
  children,
}: {
  name: string;
  color: string;
  onRemove?: () => void;
  className?: string;
  children?: React.ReactNode;
}) {
  const fg = tagTextColor(color);
  return (
    <span
      className={cn(
        "inline-flex h-7 max-w-full items-center gap-1 rounded-full px-2.5 text-xs font-medium",
        className,
      )}
      style={{ backgroundColor: color, color: fg }}
    >
      <span className="truncate">{name}</span>
      {children}
      {onRemove ? (
        <button
          type="button"
          onClick={onRemove}
          className="-mr-1 rounded-full p-0.5 opacity-70 hover:opacity-100 focus-visible:opacity-100 focus-visible:outline-2"
          style={{ outlineColor: fg }}
          aria-label={`Remove tag ${name}`}
        >
          <X className="size-3" />
        </button>
      ) : null}
    </span>
  );
}
