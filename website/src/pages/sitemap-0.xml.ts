import type { APIRoute } from "astro"

import { buildSitemap, xmlResponse } from "@/lib/sitemap"

export const GET: APIRoute = ({ site, url }) => {
  return xmlResponse(buildSitemap(site, url.origin))
}
