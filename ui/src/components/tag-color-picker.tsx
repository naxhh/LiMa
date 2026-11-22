import { Check } from "lucide-react";

import { TAG_COLOR_PRESETS, tagTextColor } from "@/lib/tags";
import { cn } from "@/lib/utils";

/** Preset swatches plus a free color input. `value` is `#RRGGBB` or null (automatic). */
export function TagColorPicker({
  value,
  onChange,
  allowAuto = false,
}: {
  value: string | null;
  onChange: (color: string | null) => void;
  allowAuto?: boolean;
}) {
  const current = value?.toUpperCase() ?? null;
  return (
    <div className="flex flex-wrap items-center gap-2">
      {allowAuto ? (
        <button
          type="button"
          onClick={() => onChange(null)}
          className={cn(
            "h-7 rounded-full border px-2.5 text-xs",
            current === null ? "border-foreground font-medium" : "text-muted-foreground",
          )}
        >
          Auto
        </button>
      ) : null}
      {TAG_COLOR_PRESETS.map((c) => (
        <button
          key={c}
          type="button"
          onClick={() => onChange(c)}
          className={cn(
            "flex size-7 items-center justify-center rounded-full ring-offset-2 ring-offset-background",
            current === c && "ring-2 ring-foreground",
          )}
          style={{ backgroundColor: c, color: tagTextColor(c) }}
          aria-label={`Color ${c}`}
          aria-pressed={current === c}
        >
          {current === c ? <Check className="size-4" /> : null}
        </button>
      ))}
      <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <input
          type="color"
          value={current ?? "#64748B"}
          onChange={(e) => onChange(e.target.value.toUpperCase())}
          className="size-7 cursor-pointer rounded border bg-transparent p-0.5"
          aria-label="Custom color"
        />
        Custom
      </label>
    </div>
  );
}
