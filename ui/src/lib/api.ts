export class ApiError extends Error {
  status: number;
  body: unknown;

  constructor(status: number, body: unknown) {
    super(`API Error ${status}`);
    this.status = status;
    this.body = body;
  }
}

async function parseBody(res: Response) {
  const ct = res.headers.get("content-type") || "";
  if (ct.includes("application/json")) {
    try {
      return await res.json();
    } catch {
      return null;
    }
  }
  try {
    return await res.text();
  } catch {
    return null;
  }
}

export async function apiGet<T>(path: string): Promise<T> {
  const res = await fetch(`/api${path}`, { headers: { accept: "application/json" } });
  if (!res.ok) throw new ApiError(res.status, await parseBody(res));
  return (await res.json()) as T;
}

export async function apiJson<T>(method: "POST" | "PATCH" | "PUT" | "DELETE", path: string, body?: unknown): Promise<T> {
  const res = await fetch(`/api${path}`, {
    method,
    headers: { "content-type": "application/json", accept: "application/json" },
    body: JSON.stringify(body),
  });

  if (!res.ok) throw new ApiError(res.status, await parseBody(res));

  const ct = res.headers.get("content-type") || "";
  if (!ct.includes("application/json")) return undefined as T;

  const text = await res.text();
  if (!text) return undefined as T;

  return JSON.parse(text) as T;
}


/** Request whose response has no body (204). Pass `body` to send JSON. */
export async function apiNoBody(method: "DELETE" | "POST" | "PUT", path: string, body?: unknown): Promise<void> {
  const res = await fetch(
    `/api${path}`,
    body === undefined
      ? { method }
      : { method, headers: { "content-type": "application/json" }, body: JSON.stringify(body) },
  );
  if (!res.ok) throw new ApiError(res.status, await parseBody(res));
}

/** Upload form for `POST /bundles`. Each file is preceded by its original date (`mtime`, ms) so it's kept. */
export function bundleForm(files: File[]): FormData {
  const form = new FormData();
  for (const f of files) {
    if (f.lastModified) form.append("mtime", String(f.lastModified));
    form.append("files[]", f, f.name);
  }
  return form;
}

export async function apiMultipart<T>(path: string, form: FormData): Promise<T> {
  const res = await fetch(`/api${path}`, {
    method: "POST",
    body: form,
  });

  if (!res.ok) throw new ApiError(res.status, await parseBody(res));
  return (await res.json()) as T;
}

export async function apiJsonNoResponse(method: "POST" | "PATCH" | "PUT", path: string, body: unknown): Promise<void> {
  const res = await fetch(`/api${path}`, {
    method,
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });

  if (!res.ok) throw new ApiError(res.status, await parseBody(res));
}
