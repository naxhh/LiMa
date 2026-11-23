/**
 * Basename of an image URL's path: query and hash are stripped, the path is
 * percent-decoded and lower-cased. Used to match a Markdown image against a
 * project's local copies by file name.
 */
export function imageBasename(url: string): string {
  const withoutQuery = url.split(/[?#]/, 1)[0];
  const last = withoutQuery.split("/").filter(Boolean).pop() ?? "";
  try {
    return decodeURIComponent(last).toLowerCase();
  } catch {
    return last.toLowerCase();
  }
}
