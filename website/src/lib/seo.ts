export const SITE_TITLE = "Satay: sans-IO OpenAPI clients for Rust"

export const SITE_DESCRIPTION =
  "Generate OpenAPI 3.1 clients for Rust without baking in reqwest or ureq. Satay writes request builders and response decoders; you send the HTTP."

export const OG_IMAGE_PATH = "/og.png"

export const OG_IMAGE_ALT =
  "Satay: OpenAPI clients without picking a transport"

export function absoluteUrl(
  path: string,
  site: URL | string | undefined,
  fallbackOrigin: string
): string {
  if (site) {
    return new URL(path, site).href
  }

  return new URL(path, fallbackOrigin).href
}
