import * as React from "react";
import { Link } from "react-router-dom";
import { MoreHorizontal } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

import { MediaImage } from "@/components/media-image";
import { thumbUrl } from "@/lib/media";

export type MediaCardAction = {
  label: string;
  onClick: () => void;
  destructive?: boolean;
  disabled?: boolean;
  separatorBefore?: boolean;
};

export type MediaCardChip = {
  label: string;
  variant?: "default" | "secondary" | "outline";
  dotColor?: string;
};

export function MediaCard(props: {
  href?: string;
  title: string;
  subtitle?: string | null;
  meta?: string | null;

  chips?: MediaCardChip[];
  actions?: MediaCardAction[];

  // If you pass `media`, it wins (e.g. 3D viewer later).
  media?: React.ReactNode;

  // If no `media`, card can render a thumbnail automatically:
  thumb?: { projectId: string; assetId: string; alt?: string; failLabel?: string };

  // If neither `media` nor `thumb` is provided, show this.
  placeholder?: React.ReactNode;
}) {
  const {
    href,
    title,
    subtitle,
    meta,
    chips = [],
    actions = [],
    media,
    thumb,
    placeholder,
  } = props;

  const hasActions = actions.length > 0;

  return (
    <div
      className={[
        "group relative rounded-2xl border bg-card overflow-hidden",
        "transition-all duration-200",
        "hover:-translate-y-0.5 hover:shadow-lg",
      ].join(" ")}
    >
      {href ? <Link to={href} className="absolute inset-0 z-0" /> : null}

      {/* content */}
      <div className="relative z-10 pointer-events-none">
        {/* media */}
        <div className="relative aspect-square bg-muted">
          {media ? (
            <div className="absolute inset-0">{media}</div>
          ) : thumb ? (
            <MediaImage
              src={thumbUrl(thumb.projectId, thumb.assetId)}
              alt={thumb.alt ?? title}
              fallbackLabel={thumb.failLabel ?? "THUMB 404"}
            />
          ) : (
            <div className="absolute inset-0 flex items-center justify-center text-xs text-muted-foreground">
              {placeholder ?? "preview"}
            </div>
          )}

          {/* gradient overlay */}
          <div className="absolute inset-0 bg-gradient-to-t from-background/80 via-background/20 to-transparent opacity-60 group-hover:opacity-80 transition-opacity" />

          {/* top-left chips */}
          {chips.length > 0 ? (
            <div className="absolute left-2 top-2 flex flex-wrap gap-2">
              {chips.map((c, i) => (
                <Badge
                  key={`${c.label}-${i}`}
                  variant={c.variant ?? "secondary"}
                  className="max-w-[10rem] truncate"
                >
                  {c.dotColor ? (
                    <span
                      className="mr-1 inline-block size-2 rounded-full align-middle"
                      style={{ backgroundColor: c.dotColor }}
                    />
                  ) : null}
                  {c.label}
                </Badge>
              ))}
            </div>
          ) : null}
        </div>

        {/* meta */}
        <div className="p-3.5 space-y-2">
          <div className="space-y-1">
            <div className="text-sm font-semibold tracking-tight truncate" title={title}>
              {title}
            </div>
            {subtitle ? (
              <div className="text-xs text-muted-foreground line-clamp-2 leading-relaxed">
                {subtitle}
              </div>
            ) : null}
          </div>

          {meta ? <div className="text-[11px] text-muted-foreground">{meta}</div> : null}
        </div>
      </div>

      {/* actions */}
      {hasActions ? (
        <div className="absolute right-2 top-2 z-20 pointer-events-auto">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="secondary"
                size="icon"
                className={[
                  "h-8 w-8 rounded-full",
                  "opacity-0 group-hover:opacity-100 transition-opacity",
                  "backdrop-blur supports-[backdrop-filter]:bg-background/60",
                ].join(" ")}
                onClick={(e) => e.stopPropagation()}
              >
                <MoreHorizontal className="h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>

            <DropdownMenuContent align="end">
              {actions.map((a, idx) => (
                <React.Fragment key={`${a.label}-${idx}`}>
                  {a.separatorBefore ? <DropdownMenuSeparator /> : null}
                  <DropdownMenuItem
                    disabled={a.disabled}
                    className={a.destructive ? "text-destructive focus:text-destructive" : undefined}
                    onClick={(e) => {
                      e.stopPropagation();
                      a.onClick();
                    }}
                  >
                    {a.label}
                  </DropdownMenuItem>
                </React.Fragment>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      ) : null}
    </div>
  );
}
