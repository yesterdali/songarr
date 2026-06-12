import { describe, expect, it, vi } from "vitest";
import { authQuery, createSession, type WaveSession } from "./auth";

describe("auth", () => {
  it("builds Subsonic token auth params", () => {
    const session: WaveSession = {
      username: "admin",
      token: "abc123",
      salt: "salt",
    };

    expect(authQuery(session)).toBe(
      "u=admin&t=abc123&s=salt&v=1.16.1&c=wave&f=json",
    );
  });

  it("creates an md5 password+salt token and trims usernames", () => {
    const getRandomValues = vi
      .spyOn(crypto, "getRandomValues")
      .mockImplementation((array) => {
        (array as Uint8Array).fill(1);
        return array;
      });

    const session = createSession(" admin ", "songarr-test");

    expect(session.username).toBe("admin");
    expect(session.salt).toBe("01010101010101010101010101010101");
    expect(session.token).toBe("58b4fd15573fcf88e875fb4614fbb4dc");

    getRandomValues.mockRestore();
  });
});
