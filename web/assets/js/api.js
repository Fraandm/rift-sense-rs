/* Cliente HTTP minimo con errores legibles. */

async function request(path, options = {}) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), options.timeout ?? 30000);
  const signal = options.signal
    ? AbortSignal.any([options.signal, controller.signal])
    : controller.signal;

  try {
    const res = await fetch(path, {
      method: options.method || "GET",
      headers: { "Content-Type": "application/json", ...(options.headers || {}) },
      body: options.body ? JSON.stringify(options.body) : undefined,
      signal,
    });
    const text = await res.text();
    let data = null;
    try { data = text ? JSON.parse(text) : null; } catch { data = null; }
    if (!res.ok) {
      const detail = data && (data.detail || data.error);
      const msg = typeof detail === "string" ? detail : `Error ${res.status}`;
      throw new Error(msg);
    }
    return data;
  } catch (err) {
    if (err.name === "AbortError") {
      const e = new Error("La petición tardó demasiado. Reintenta.");
      e.name = "AbortError";
      throw e;
    }
    throw err;
  } finally {
    clearTimeout(timeout);
  }
}

export const api = {
  get: (path, options) => request(path, { ...options, method: "GET" }),
  post: (path, body, options) => request(path, { ...options, method: "POST", body }),
};
