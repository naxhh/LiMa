export class ApiError extends Error {
  status: number;
  body: unknown;

  constructor(status: number, body: unknown) {
    super(`API Error ${status}`);
    this.status = status;
    this.body = body;
  }
}

export async function apiGet<T>(path: string): Promise<T> {
  const res = await fetch(`/api${path}`, {
    headers: { accept: "application/json" },
  });

  if (!res.ok) {
    let body: unknown = null;
    try { body = await res.json(); } catch {}
    throw new ApiError(res.status, body);
  }

  return (await res.json()) as T;
}
