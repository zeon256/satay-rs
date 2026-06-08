export const SITE_TITLE = "Satay: sans-IO OpenAPI clients for Rust"

export const SITE_DESCRIPTION =
  "Satay generates sans-IO OpenAPI clients for Rust: request builders, response decoders, and validation newtypes. You pick the HTTP stack."

export const OG_IMAGE_PATH = "/og.png"

export const OG_IMAGE_ALT = "Satay — OpenAPI clients without picking a transport"

export function absoluteUrl(path: string, site: URL | string | undefined, fallbackOrigin: string): string {
  if (site) {
    return new URL(path, site).href
  }

  return new URL(path, fallbackOrigin).href
}
