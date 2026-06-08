export type SatayUser = {
  name: string
  description: string
  href: string
  repo: string
  /** Site path (e.g. /users/nea-rs.webp) or absolute URL. */
  logo?: string
  logoAlt?: string
  crates?: string
  docs?: string
}

/** Directory for project logos. Keep in sync with USERS_SUBMISSION_URL. */
export const USERS_LOGO_DIR = "website/public/users"

/** Path in the satay-rs repo. Keep in sync with USERS_SUBMISSION_URL. */
export const USERS_FILE_PATH = "website/src/data/users.ts"

export const USERS_SUBMISSION_URL =
  "https://github.com/zeon256/satay-rs/edit/main/website/src/data/users.ts"

export const satayUsers: SatayUser[] = [
  {
    name: "InfiniteUnion/nea-rs",
    description:
      "Type-safe, sans-IO Rust client for Singapore NEA weather and environmental APIs.",
    href: "https://github.com/InfiniteUnion/nea-rs",
    repo: "https://github.com/InfiniteUnion/nea-rs",
    logo: "/users/nea-rs.webp",
    crates: "https://crates.io/crates/nea-rs",
    docs: "https://docs.rs/nea-rs",
  },
]
