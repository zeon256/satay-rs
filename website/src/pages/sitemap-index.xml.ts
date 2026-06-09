import type { APIRoute } from "astro"

import { buildSitemapIndex, xmlResponse } from "@/lib/sitemap"

export const GET: APIRoute = ({ site, url }) => {
  return xmlResponse(buildSitemapIndex(site, url.origin))
}
