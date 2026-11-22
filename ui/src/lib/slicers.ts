/**
 * "Open in slicer": hands a model's download URL to a desktop slicer through its URL scheme.
 * Slicers may refuse URLs from domains they don't trust (see docs/SLICERS.md), so every
 * attempt also offers a plain download.
 */
import { toast } from "sonner";

export type Slicer = {
  id: "bambu" | "orca" | "prusa";
  name: string;
  /** Lower-case extensions it opens. */
  formats: string[];
  link: (fileUrl: string) => string;
};

export const SLICERS: Slicer[] = [
  {
    id: "bambu",
    name: "Bambu Studio",
    formats: ["stl", "3mf", "obj", "step", "stp"],
    link: (url) => `bambustudio://open?file=${encodeURIComponent(url)}`,
  },
  {
    id: "orca",
    name: "OrcaSlicer",
    formats: ["stl", "3mf", "obj", "step", "stp"],
    link: (url) => `orcaslicer://open?file=${encodeURIComponent(url)}`,
  },
  {
    id: "prusa",
    name: "PrusaSlicer",
    formats: ["stl", "3mf", "obj", "step", "stp"],
    link: (url) => `prusaslicer://open?file=${encodeURIComponent(url)}`,
  },
];

const KEY = "lima.slicer";

export function preferredSlicer(): Slicer | null {
  try {
    const id = localStorage.getItem(KEY);
    return SLICERS.find((s) => s.id === id) ?? null;
  } catch {
    return null;
  }
}

export function setPreferredSlicer(slicer: Slicer) {
  try {
    localStorage.setItem(KEY, slicer.id);
  } catch {
    // The choice just isn't remembered.
  }
}

export function extensionOf(path: string): string {
  const name = path.split("/").pop() ?? "";
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/** Whether any slicer can open this file. */
export function sliceable(path: string): boolean {
  const ext = extensionOf(path);
  return SLICERS.some((s) => s.formats.includes(ext));
}

/** Starts a normal browser download. */
export function downloadFile(url: string, name: string) {
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
}

/**
 * Opens `fileUrl` (relative to LiMa) in `slicer`, remembers it as the preferred one, and
 * shows a toast with a download fallback in case the slicer isn't installed or refuses the link.
 */
export function openInSlicer(slicer: Slicer, fileUrl: string, fileName: string) {
  setPreferredSlicer(slicer);
  const absolute = new URL(fileUrl, window.location.origin).href;
  window.location.href = slicer.link(absolute);
  toast(`Opening ${fileName} in ${slicer.name}…`, {
    description: "Nothing happened? The slicer may not be installed, or may not accept links from this address.",
    duration: 8000,
    action: { label: "Download", onClick: () => downloadFile(fileUrl, fileName) },
  });
}
