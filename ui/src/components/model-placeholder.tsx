import { cn } from "@/lib/utils";

/** Dot-matrix tile with a wireframe box, shown where there is no image. */
export function ModelPlaceholder({ className, label }: { className?: string; label?: string }) {
  return (
    <div className={cn("bg-dots absolute inset-0 grid place-items-center bg-thumb", className)}>
      <svg
        viewBox="0 0 100 100"
        className="w-[42%] max-w-40 text-signal"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.2"
        strokeLinejoin="round"
        aria-hidden
      >
        <path d="M50 10 88 30v40L50 90 12 70V30z" />
        <path d="M12 30l38 20 38-20M50 50v40" />
        <path d="M31 20l38 20M69 20 31 40" strokeDasharray="2 3" opacity=".55" />
      </svg>
      {label ? (
        <span className="absolute bottom-2.5 left-1/2 -translate-x-1/2 font-mono text-[10px] tracking-[0.1em] text-dim uppercase">
          {label}
        </span>
      ) : null}
    </div>
  );
}
