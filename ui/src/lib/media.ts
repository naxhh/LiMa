export function joinUrlSegments(...segs: string[]) {
  return segs
    .flatMap((s) => s.split("/"))
    .filter(Boolean)
    .map(encodeURIComponent)
    .join("/");
}

export function thumbUrl(projectId: string, assetId: string) {
  return `/media/thumbs/${encodeURIComponent(projectId)}/${encodeURIComponent(assetId)}.jpg`;
}

/** Decimated mesh built by the server for the 3D viewer (see `crates/indexer/src/preview_mesh.rs`). */
export function previewMeshUrl(projectId: string, assetId: string) {
  return `/api/projects/${encodeURIComponent(projectId)}/assets/${encodeURIComponent(assetId)}/preview`;
}

export function libraryUrl(projectFolderPath: string, filePath: string) {
  return `/media/library/${joinUrlSegments(projectFolderPath, filePath)}`;
}

/** Zip of every file in the project (streamed by the server). */
export function projectDownloadUrl(projectId: string) {
  return `/api/projects/${encodeURIComponent(projectId)}/download`;
}

/** A file inside a zip asset, streamed out of the archive. */
export function zipEntryUrl(projectId: string, assetId: string, path: string, download = false) {
  const q = new URLSearchParams({ path });
  if (download) q.set("download", "true");
  return `/api/projects/${encodeURIComponent(projectId)}/assets/${encodeURIComponent(assetId)}/zip/entry?${q}`;
}
