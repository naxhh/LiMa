/** Formats the in-browser 3D viewer reads (STEP needs a CAD kernel, so it isn't one). */
export type ViewerFormat = "stl" | "obj" | "3mf";

const FORMATS: ViewerFormat[] = ["stl", "obj", "3mf"];

/** The viewer format for a file name, or null if it can't be previewed in the browser. */
export function viewerFormat(path: string): ViewerFormat | null {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return (FORMATS as string[]).includes(ext) ? (ext as ViewerFormat) : null;
}
