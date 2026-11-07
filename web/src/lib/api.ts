// Tiny fetch wrapper. All URLs are relative so the app works behind a
// reverse proxy at any sub-path (e.g. https://example.com/kinora/).

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  // A Blob (an image upload) goes as-is; anything else as JSON.
  const raw = body instanceof Blob;
  const res = await fetch(path.replace(/^\//, ""), {
    method,
    credentials: "same-origin",
    headers: body === undefined ? undefined : { "content-type": raw ? body.type : "application/json" },
    body: body === undefined ? undefined : raw ? body : JSON.stringify(body),
  });
  if (!res.ok) {
    let message = res.statusText;
    try {
      const data = await res.json();
      if (data?.error) message = data.error;
    } catch {
      // not JSON
    }
    throw new ApiError(res.status, message);
  }
  if (res.status === 204) return undefined as T;
  const text = await res.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

export const api = {
  get: <T>(path: string) => request<T>("GET", path),
  post: <T>(path: string, body?: unknown) => request<T>("POST", path, body ?? {}),
  put: <T>(path: string, body?: unknown) => request<T>("PUT", path, body ?? {}),
  patch: <T>(path: string, body?: unknown) => request<T>("PATCH", path, body ?? {}),
  del: <T>(path: string, body?: unknown) => request<T>("DELETE", path, body),
};

/** A user's profile picture, or null without one. The version makes a new picture skip the cache. */
export const avatarUrl = (user: { id: string; avatar_version?: number | null }) =>
  user.avatar_version ? `api/users/${user.id}/avatar?v=${user.avatar_version}` : null;

export const imageUrl = (id: string, kind: "poster" | "backdrop" | "thumb") => `api/items/${id}/images/${kind}`;
