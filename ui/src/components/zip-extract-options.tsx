import type { ZipPrefs } from "@/lib/zip-prefs";

/** "Extract zip files" for uploads and imports that include zips. */
export function ZipExtractOptions({ count, value, onChange }: { count: number; value: ZipPrefs; onChange: (next: ZipPrefs) => void }) {
  if (count === 0) return null;
  return (
    <div className="space-y-2 rounded-xl border bg-secondary/30 p-3 text-sm">
      <label className="flex cursor-pointer items-start gap-2.5">
        <input
          type="checkbox"
          className="mt-0.5 size-4 shrink-0 accent-primary"
          checked={value.extract}
          onChange={(e) => onChange({ ...value, extract: e.target.checked })}
        />
        <span>
          <span className="font-medium">Extract {count === 1 ? "the zip file" : `the ${count} zip files`}</span>
          <span className="block text-xs text-muted-foreground">Their files are added to the project, keeping the archive’s folders.</span>
        </span>
      </label>
      {value.extract ? (
        <label className="flex cursor-pointer items-center gap-2.5 pl-6.5">
          <input
            type="checkbox"
            className="size-4 shrink-0 accent-primary"
            checked={value.keep}
            onChange={(e) => onChange({ ...value, keep: e.target.checked })}
          />
          <span className="text-muted-foreground">Keep the original zip too</span>
        </label>
      ) : null}
    </div>
  );
}
