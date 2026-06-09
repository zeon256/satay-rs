import type { APIRoute } from "astro"

import { SITEMAP_INDEX_PATH } from "@/lib/sitemap"

const ALLOWED_PATHS = [
  "/",
  "/fonts/",
  "/users/",
  "/favicon.svg",
  "/logo.webp",
  "/og.png",
  "/og.svg",
] as const

const CONTENT_SIGNAL = "Content-Signal: ai-train=yes, search=yes, ai-input=yes"

function buildRobotsTxt(sitemapUrl: URL | undefined): string {
  const allowRules = ALLOWED_PATHS.map((path) => `Allow: ${path}`).join("\n")

  const lines = [
    "User-agent: *",
    CONTENT_SIGNAL,
    allowRules,
    "Disallow: /_astro/",
    "",
    "User-agent: Googlebot",
    "Allow: /",
    "",
    "User-agent: Bingbot",
    "Allow: /",
  ]

  if (sitemapUrl) {
    lines.push("", `Sitemap: ${sitemapUrl.href}`)
  }

  return lines.join("\n")
}

export const GET: APIRoute = ({ site }) => {
  const sitemapUrl = site ? new URL(SITEMAP_INDEX_PATH, site) : undefined

  return new Response(buildRobotsTxt(sitemapUrl), {
    headers: {
      "Content-Type": "text/plain; charset=utf-8",
    },
  })
}
