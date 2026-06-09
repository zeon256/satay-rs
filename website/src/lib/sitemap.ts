import { absoluteUrl } from "@/lib/seo"

export const SITEMAP_INDEX_PATH = "/sitemap-index.xml"
export const SITEMAP_PATH = "/sitemap-0.xml"

/** Indexable site pages. Add routes here when new pages ship. */
export const SITEMAP_PAGES = ["/"] as const

const XML_HEADERS = {
  "Content-Type": "application/xml; charset=utf-8",
} as const

export function xmlResponse(body: string): Response {
  return new Response(body, { headers: XML_HEADERS })
}

export function buildSitemapIndex(
  site: URL | string | undefined,
  fallbackOrigin: string
): string {
  const sitemapUrl = absoluteUrl(SITEMAP_PATH, site, fallbackOrigin)

  return `<?xml version="1.0" encoding="UTF-8"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <sitemap>
    <loc>${sitemapUrl}</loc>
  </sitemap>
</sitemapindex>`
}

export function buildSitemap(
  site: URL | string | undefined,
  fallbackOrigin: string
): string {
  const urls = SITEMAP_PAGES.map(
    (path) => `  <url><loc>${absoluteUrl(path, site, fallbackOrigin)}</loc></url>`
  ).join("\n")

  return `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${urls}
</urlset>`
}
