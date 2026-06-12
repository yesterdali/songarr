import md5 from "blueimp-md5";

export type WaveSession = {
  username: string;
  token: string;
  salt: string;
};

const SESSION_KEY = "songarr.wave.session.v1";

export function generateSalt(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join(
    "",
  );
}

export function createSession(username: string, password: string): WaveSession {
  const salt = generateSalt();
  return {
    username: username.trim(),
    token: md5(`${password}${salt}`),
    salt,
  };
}

export function authParams(session: WaveSession, format = "json"): URLSearchParams {
  const params = new URLSearchParams({
    u: session.username,
    t: session.token,
    s: session.salt,
    v: "1.16.1",
    c: "wave",
  });
  if (format) {
    params.set("f", format);
  }
  return params;
}

export function authQuery(session: WaveSession, format = "json"): string {
  return authParams(session, format).toString();
}

export function loadSession(): WaveSession | null {
  try {
    const raw = localStorage.getItem(SESSION_KEY);
    if (!raw) {
      return null;
    }
    const parsed = JSON.parse(raw) as Partial<WaveSession>;
    if (!parsed.username || !parsed.token || !parsed.salt) {
      return null;
    }
    return {
      username: parsed.username,
      token: parsed.token,
      salt: parsed.salt,
    };
  } catch {
    return null;
  }
}

export function saveSession(session: WaveSession): void {
  localStorage.setItem(SESSION_KEY, JSON.stringify(session));
}

export function clearSession(): void {
  localStorage.removeItem(SESSION_KEY);
}

export async function validateSession(session: WaveSession): Promise<void> {
  const response = await fetch(`/rest/ping?${authQuery(session)}`, {
    headers: { Accept: "application/json" },
  });
  if (!response.ok) {
    throw new Error(`Server returned HTTP ${response.status}`);
  }
  const body = (await response.json()) as {
    "subsonic-response"?: { status?: string; error?: { message?: string } };
  };
  const subsonic = body["subsonic-response"];
  if (subsonic?.status !== "ok") {
    throw new Error(subsonic?.error?.message ?? "Login failed");
  }
}
