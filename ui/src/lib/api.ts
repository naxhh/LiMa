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

export async function apiJson<T>(method: "POST" | "PATCH" | "PUT", path: string, body: unknown): Promise<T> {
  const res = await fetch(`/api${path}`, {
    method,
    headers: { "content-type": "application/json", accept: "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new ApiError(res.status, await parseBody(res));
  return (await res.json()) as T;
}

export async function apiNoBody(method: "DELETE" | "POST", path: string): Promise<void> {
  const res = await fetch(`/api${path}`, { method });
  if (!res.ok) throw new ApiError(res.status, await parseBody(res));
}
