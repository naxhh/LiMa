import { Fragment } from "react";
import { Link } from "react-router-dom";

export type PromptSegment = { label: string; to?: string };

/**
 * A CLI-style breadcrumb: `lima@~/library/model $` with a blinking block cursor.
 * The first segment carries the `~/` prefix; later ones are joined with `/`.
 */
export function PromptBreadcrumb({ segments }: { segments: PromptSegment[] }) {
  return (
    <nav aria-label="Breadcrumb" className="flex min-w-0 items-center gap-1.5 font-mono text-xs">
      <span className="shrink-0 text-signal">lima@</span>
      {segments.map((seg, i) => (
        <Fragment key={`${seg.label}-${i}`}>
          {i > 0 ? <span className="shrink-0 text-dim">/</span> : null}
          {seg.to ? (
            <Link to={seg.to} className="shrink-0 text-muted-foreground hover:text-signal">
              {seg.label}
            </Link>
          ) : (
            <span className="min-w-0 truncate text-foreground">{seg.label}</span>
          )}
        </Fragment>
      ))}
      <span className="shrink-0 text-dim">$</span>
      <span className="inline-block h-3.5 w-[7px] shrink-0 translate-y-[2px] animate-pulse bg-signal/70 motion-reduce:animate-none" />
    </nav>
  );
}
