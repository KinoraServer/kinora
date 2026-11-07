// A tiny stand-in for the Kinora server, so stories (and the Storybook tests)
// render without one. It swaps window.fetch for the length of a story and
// answers `api/...` paths from a table.
//
// Stories add or override answers with `parameters.api`:
//
//   parameters: { api: { "api/player/style": { style: "pro" } } }
//
// A value can be JSON, a function of the request, or `mockError(status)`.

import { builtinThemes, demoLibraries, demoSystemInfo, demoUser } from "./fixtures";

export type MockHandler = unknown | ((req: { path: string; method: string; body: unknown }) => unknown);
export type MockApi = Record<string, MockHandler>;

const ERROR = Symbol("mock-error");
interface MockError {
  [ERROR]: true;
  status: number;
  message: string;
}

/** Answer with an HTTP error instead of JSON. */
export function mockError(status: number, message = "Mocked error"): MockError {
  return { [ERROR]: true, status, message };
}

/** Answers every story gets unless it overrides them. */
export function defaultApi(themeId: string): MockApi {
  return {
    "api/themes": { default_theme: themeId, themes: builtinThemes },
    "api/player/style": { style: "theme" },
    "api/player-skins": [],
    "api/system/info": demoSystemInfo,
    "api/auth/me": demoUser,
    "api/libraries": demoLibraries,
  };
}

const json = (status: number, body: unknown) =>
  new Response(body === undefined ? null : JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });

/** Normalise "/api/x?y=1", "./api/x" and "http://host/api/x" to "api/x". */
function apiPath(input: RequestInfo | URL): string | null {
  const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
  const url = new URL(raw, window.location.href);
  const i = url.pathname.indexOf("/api/");
  if (i === -1) return null;
  return url.pathname.slice(i + 1);
}

/** Replace window.fetch with the mock. Returns a function that puts the real one back. */
export function installMockApi(api: MockApi): () => void {
  const realFetch = window.fetch;
  window.fetch = async (input, init) => {
    const path = apiPath(input);
    if (path === null) return realFetch(input, init);
    const method = (init?.method ?? "GET").toUpperCase();
    const key = [`${method} ${path}`, path].find((k) => k in api);
    if (!key) {
      console.warn(`[mock-api] no mock for ${method} ${path}`);
      return json(404, { error: `No mock for ${method} ${path}` });
    }
    const handler = api[key];
    let body: unknown;
    try {
      body = typeof init?.body === "string" ? JSON.parse(init.body) : undefined;
    } catch {
      body = init?.body;
    }
    const value = typeof handler === "function" ? await handler({ path, method, body }) : handler;
    if (value && typeof value === "object" && ERROR in value) {
      const err = value as MockError;
      return json(err.status, { error: err.message });
    }
    return json(200, value);
  };
  return () => {
    window.fetch = realFetch;
  };
}
