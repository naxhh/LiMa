import * as React from "react";

type Props = {
  files: File[];
  onFiles: (files: File[]) => void;
  accept?: string;
};

export function FileDropzone({ files, onFiles, accept }: Props) {
  const inputRef = React.useRef<HTMLInputElement | null>(null);
  const [dragOver, setDragOver] = React.useState(false);

  function addFiles(list: File[]) {
    if (list.length === 0) return;

    // de-dupe by name+size+mtime
    const key = (f: File) => `${f.name}::${f.size}::${f.lastModified}`;
    const existing = new Set(files.map(key));

    const merged = [...files];
    for (const f of list) {
      if (!existing.has(key(f))) {
        merged.push(f);
        existing.add(key(f));
      }
    }
    onFiles(merged);
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

          const list = Array.from(e.dataTransfer.files || []);
          addFiles(list);
        }}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") inputRef.current?.click();
        }}
      >
        <div className="font-medium">Drag & drop files here</div>
        <div className="text-muted-foreground">
          or click to choose files
        </div>
      </div>

      <input
        ref={inputRef}
        type="file"
        multiple
        accept={accept}
        className="hidden"
        onChange={(e) => {
          const list = e.target.files ? Array.from(e.target.files) : [];
          addFiles(list);
          // allow selecting same file again later
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
