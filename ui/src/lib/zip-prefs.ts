import * as React from "react";

/** Whether uploads extract their zip files, remembered per browser. Off by default. */
export type ZipPrefs = { extract: boolean; keep: boolean };

const KEY = "lima.zipExtract";

function read(): ZipPrefs {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (v && typeof v.extract === "boolean" && typeof v.keep === "boolean") return v;
  } catch {
    /* storage unavailable */
  }
  return { extract: false, keep: false };
}

export function useZipPrefs(): [ZipPrefs, (next: ZipPrefs) => void] {
  const [prefs, setPrefs] = React.useState(read);
  const update = (next: ZipPrefs) => {
    setPrefs(next);
    try {
      localStorage.setItem(KEY, JSON.stringify(next));
    } catch {
      /* storage unavailable */
    }
  };
  return [prefs, update];
}

export function hasZip(names: string[]): boolean {
  return names.some((n) => n.toLowerCase().endsWith(".zip"));
}

/** The import request fields for these preferences. */
export function zipFields(prefs: ZipPrefs, names: string[]) {
  return hasZip(names) && prefs.extract ? { extract_zips: true, keep_zips: prefs.keep } : {};
}
