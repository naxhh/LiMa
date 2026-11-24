import * as React from "react";

type Props = {
  files: File[];
  onFiles: (files: File[]) => void;
  accept?: string;
};

/** A file found in a drop or a folder pick, with the folders it sat in. */
type Found = { file: File; dirs: string[] };

const JUNK_NAMES = new Set([".ds_store", "thumbs.db", "desktop.ini"]);

/** Original `name::size::mtime` for files renamed to avoid a clash, so the same drop de-dupes. */
const originalKey = new WeakMap<File, string>();

/** OS junk that comes along with folders: never upload it. Dotfiles and dotfolders count too. */
function isJunk(name: string, isDirectory: boolean): boolean {
  if (name.startsWith(".")) return true;
  if (isDirectory && name.toLowerCase() === "__macosx") return true;
  return !isDirectory && JUNK_NAMES.has(name.toLowerCase());
}

/** Every entry in a directory; `readEntries` only hands back one batch at a time. */
function readDirectory(dir: FileSystemDirectoryEntry): Promise<FileSystemEntry[]> {
  const reader = dir.createReader();
  const all: FileSystemEntry[] = [];
  return new Promise((resolve, reject) => {
    const next = () =>
      reader.readEntries((batch) => {
        if (batch.length === 0) return resolve(all);
        all.push(...batch);
        next();
      }, reject);
    next();
  });
}

/** Walks dropped entries recursively, keeping the folder names of every file. */
async function collectEntries(entries: FileSystemEntry[], dirs: string[] = []): Promise<Found[]> {
  const out: Found[] = [];
  for (const entry of entries) {
    if (isJunk(entry.name, entry.isDirectory)) continue;
    if (entry.isFile) {
      const file = await new Promise<File>((resolve, reject) => (entry as FileSystemFileEntry).file(resolve, reject));
      out.push({ file, dirs });
    } else if (entry.isDirectory) {
      const children = await readDirectory(entry as FileSystemDirectoryEntry);
      out.push(...(await collectEntries(children, [...dirs, entry.name])));
    }
  }
  return out;
}

/** Bundles are flat and the server rejects `/` in names, so files keep their own name unless two clash. */
function uniqueName(name: string, dirs: string[], used: Set<string>): string {
  if (!used.has(name)) return name;
  const prefixed = dirs.length > 0 ? `${dirs.join("-")}-${name}` : name;
  if (!used.has(prefixed)) return prefixed;
  const dot = prefixed.lastIndexOf(".");
  const stem = dot > 0 ? prefixed.slice(0, dot) : prefixed;
  const ext = dot > 0 ? prefixed.slice(dot) : "";
  for (let n = 2; ; n++) {
    const candidate = `${stem}-${n}${ext}`;
    if (!used.has(candidate)) return candidate;
  }
}

/** Folders a picked file sat in, from `webkitRelativePath` (`parts/base.stl` → `["parts"]`). */
function pickedDirs(file: File): string[] {
  if (!file.webkitRelativePath) return [];
  const parts = file.webkitRelativePath.split("/").filter(Boolean);
  parts.pop();
  return parts;
}

export function FileDropzone({ files, onFiles, accept }: Props) {
  const inputRef = React.useRef<HTMLInputElement | null>(null);
  const folderRef = React.useRef<HTMLInputElement | null>(null);
  const [dragOver, setDragOver] = React.useState(false);

  function addFound(list: Found[]) {
    if (list.length === 0) return;

    // de-dupe by name+size+mtime, keeping the original name after a flatten rename
    const key = (f: File) => `${f.name}::${f.size}::${f.lastModified}`;
    const identity = (f: File) => originalKey.get(f) ?? key(f);
    const existing = new Set(files.map(identity));
    const used = new Set(files.map((f) => f.name));

    const merged = [...files];
    for (const { file, dirs } of list) {
      const id = key(file);
      if (existing.has(id)) continue;
      const name = uniqueName(file.name, dirs, used);
      const chosen = name === file.name ? file : new File([file], name, { type: file.type, lastModified: file.lastModified });
      originalKey.set(chosen, id);
      merged.push(chosen);
      existing.add(id);
      used.add(name);
    }
    onFiles(merged);
  }

  /** Files from the folder picker, flattened the same way as drops. */
  function addPicked(list: File[]) {
    addFound(
      list
        .map((file) => ({ file, dirs: pickedDirs(file) }))
        .filter(({ file, dirs }) => !dirs.some((d) => isJunk(d, true)) && !isJunk(file.name, false)),
    );
  }

  return (
    <div className="space-y-2">
      <div
        className={[
          "rounded-lg border border-dashed p-6 text-sm",
          dragOver ? "bg-muted/60" : "bg-muted/20",
        ].join(" ")}
        onClick={() => inputRef.current?.click()}
        onDragEnter={(e) => {
          e.preventDefault();
          e.stopPropagation();
          setDragOver(true);
        }}
        onDragOver={(e) => {
          e.preventDefault();
          e.stopPropagation();
          setDragOver(true);
        }}
        onDragLeave={(e) => {
          e.preventDefault();
          e.stopPropagation();
          setDragOver(false);
        }}
        onDrop={(e) => {
          e.preventDefault();
          e.stopPropagation();
          setDragOver(false);

          // `getAsEntry` is only valid during the event, so read every item before awaiting.
          const items = Array.from(e.dataTransfer.items ?? []);
          const entries: FileSystemEntry[] = [];
          const loose: File[] = [];
          for (const item of items) {
            if (item.kind !== "file") continue;
            const entry = item.webkitGetAsEntry?.() ?? null;
            if (entry) entries.push(entry);
            else {
              const file = item.getAsFile();
              if (file) loose.push(file);
            }
          }

          if (entries.length === 0) {
            addFound(Array.from(e.dataTransfer.files ?? []).map((file) => ({ file, dirs: [] })));
            return;
          }
          void collectEntries(entries)
            .then((found) => addFound([...found, ...loose.map((file) => ({ file, dirs: [] }))]))
            .catch(() => addFound(loose.map((file) => ({ file, dirs: [] }))));
        }}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") inputRef.current?.click();
        }}
      >
        <div className="font-medium">Drag & drop files or folders here</div>
        <div className="text-muted-foreground">
          or click to choose files
        </div>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="text-xs text-muted-foreground">Folders are added with every file inside them.</span>
        <button
          type="button"
          className="text-xs text-muted-foreground underline hover:text-foreground"
          onClick={() => folderRef.current?.click()}
        >
          Choose a folder
        </button>
      </div>

      <input
        ref={inputRef}
        type="file"
        multiple
        accept={accept}
        className="hidden"
        onChange={(e) => {
          const list = e.target.files ? Array.from(e.target.files) : [];
          addFound(list.map((file) => ({ file, dirs: [] })));
          // allow selecting same file again later
          e.currentTarget.value = "";
        }}
      />

      <input
        ref={folderRef}
        type="file"
        multiple
        className="hidden"
        {...({ webkitdirectory: "", directory: "" } as Record<string, string>)}
        onChange={(e) => {
          const list = e.target.files ? Array.from(e.target.files) : [];
          addPicked(list);
          // allow picking the same folder again later
          e.currentTarget.value = "";
        }}
      />

      {files.length > 0 ? (
        <div className="rounded-lg border p-3">
          <div className="text-xs text-muted-foreground mb-2">
            {files.length} file(s) selected
          </div>
          <ul className="space-y-1">
            {files.map((f) => (
              <li key={`${f.name}:${f.size}:${f.lastModified}`} className="flex items-center justify-between gap-2">
                <span className="truncate">{f.name}</span>
                <span className="text-xs text-muted-foreground">{Math.round(f.size / 1024)} KB</span>
              </li>
            ))}
          </ul>

          <div className="pt-2">
            <button
              className="text-xs underline text-muted-foreground hover:text-foreground"
              onClick={(e) => {
                e.preventDefault();
                onFiles([]);
              }}
            >
              Clear
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
