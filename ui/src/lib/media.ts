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

export function libraryUrl(projectFolderPath: string, filePath: string) {
  console.log("libraryUrl", { projectFolderPath, filePath });
  return `/media/library/${joinUrlSegments(projectFolderPath, filePath)}`;
}
