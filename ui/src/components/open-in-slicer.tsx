import * as React from "react";
import { ChevronDown, Download, Printer } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { SLICERS, downloadFile, extensionOf, openInSlicer, preferredSlicer, setPreferredSlicer, type Slicer } from "@/lib/slicers";

export type SliceableFile = { url: string; name: string };

/** One file: icon button that opens it in the preferred slicer, menu for the others. Used in file rows. */
export function OpenFileInSlicer({ file }: { file: SliceableFile }) {
  const [preferred, setPreferred] = React.useState<Slicer | null>(preferredSlicer);
  const ext = extensionOf(file.name);
  const slicers = SLICERS.filter((s) => s.formats.includes(ext));
  const open = (s: Slicer) => {
    openInSlicer(s, file.url, file.name);
    setPreferred(s);
  };

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" size="icon-sm" aria-label={`Open ${file.name} in a slicer`} title="Open in slicer">
          <Printer />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuLabel className="font-mono text-[10px] tracking-[0.1em] text-dim uppercase">Open in</DropdownMenuLabel>
        {slicers.map((s) => (
          <DropdownMenuItem key={s.id} onClick={() => open(s)}>
            {s.name}
            {preferred?.id === s.id ? <span className="ml-auto font-mono text-[10px] text-dim">default</span> : null}
          </DropdownMenuItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={() => downloadFile(file.url, file.name)}>
          <Download />
          Download instead
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/**
 * Project-level button. With a preferred slicer and one model it's a single click;
 * otherwise the menu lists the models and lets you pick the slicer.
 */
export function OpenProjectInSlicer({ files }: { files: SliceableFile[] }) {
  const [preferred, setPreferred] = React.useState<Slicer | null>(preferredSlicer);
  if (files.length === 0) return null;
  const slicer = preferred ?? SLICERS[0];
  const open = (file: SliceableFile, s: Slicer = slicer) => {
    openInSlicer(s, file.url, file.name);
    setPreferred(s);
  };
  const single = files.length === 1 && preferred;

  return (
    <div className="flex">
      {single ? (
        <Button variant="outline" className="rounded-r-none border-r-0" onClick={() => open(files[0])}>
          <Printer />
          Open in {preferred.name}
        </Button>
      ) : null}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          {single ? (
            <Button variant="outline" size="icon" className="rounded-l-none" aria-label="Choose slicer">
              <ChevronDown />
            </Button>
          ) : (
            <Button variant="outline">
              <Printer />
              Open in slicer
              <ChevronDown className="opacity-60" />
            </Button>
          )}
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="max-w-[min(22rem,calc(100vw-2rem))]">
          <DropdownMenuLabel className="font-mono text-[10px] tracking-[0.1em] text-dim uppercase">
            {files.length === 1 ? "Open in" : `Open in ${slicer.name}`}
          </DropdownMenuLabel>
          {files.length === 1
            ? SLICERS.map((s) => (
                <DropdownMenuItem key={s.id} onClick={() => open(files[0], s)}>
                  {s.name}
                </DropdownMenuItem>
              ))
            : files.map((f) => (
                <DropdownMenuItem key={f.url} onClick={() => open(f)}>
                  <span className="truncate">{f.name}</span>
                </DropdownMenuItem>
              ))}
          {files.length > 1 ? (
            <>
              <DropdownMenuSeparator />
              <DropdownMenuLabel className="font-mono text-[10px] tracking-[0.1em] text-dim uppercase">Slicer</DropdownMenuLabel>
              <DropdownMenuRadioGroup
                value={slicer.id}
                onValueChange={(id) => {
                  const s = SLICERS.find((x) => x.id === id);
                  if (!s) return;
                  setPreferredSlicer(s);
                  setPreferred(s);
                }}
              >
                {SLICERS.map((s) => (
                  <DropdownMenuRadioItem key={s.id} value={s.id} onSelect={(e) => e.preventDefault()}>
                    {s.name}
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
            </>
          ) : null}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
