import { test, expect } from "@playwright/test";
import { request as playwrightRequest } from "@playwright/test";

test("session cookie has HttpOnly, SameSite=Strict, and no Domain attribute", async ({
  baseURL,
}) => {
  const ctx = await playwrightRequest.newContext({
    baseURL,
    ignoreHTTPSErrors: true,
    maxRedirects: 0,
  });

  const response = await ctx.get("/ui/zones");

  expect(response.status()).toBeGreaterThanOrEqual(300);
  expect(response.status()).toBeLessThan(400);

  const setCookie = response.headers()["set-cookie"];
  expect(setCookie).toBeDefined();

  const cookieStr = Array.isArray(setCookie) ? setCookie.join("; ") : setCookie;
  expect(cookieStr).toContain("HttpOnly");
  expect(cookieStr).toContain("SameSite=Strict");
  expect(cookieStr).not.toMatch(/Domain=/i);

  await ctx.dispose();
});
