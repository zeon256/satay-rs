const USER_AGENT = "satay-website/0.0.1 (https://github.com/zeon256/satay-rs)"

export type ProjectStats = {
  crateDownloads: number | null
  githubStars: number | null
}

type CratesResponse = {
  crate?: {
    downloads?: number
  }
}

type GithubRepoResponse = {
  stargazers_count?: number
}

export async function fetchProjectStats(): Promise<ProjectStats> {
  const [crateDownloads, githubStars] = await Promise.all([
    fetchCrateDownloads("satay-cli"),
    fetchGithubStars("zeon256", "satay-rs"),
  ])

  return { crateDownloads, githubStars }
}

async function fetchCrateDownloads(crate: string): Promise<number | null> {
  try {
    const response = await fetch(`https://crates.io/api/v1/crates/${crate}`, {
      headers: { "User-Agent": USER_AGENT },
    })

    if (!response.ok) return null

    const data = (await response.json()) as CratesResponse
    const downloads = data.crate?.downloads

    return typeof downloads === "number" ? downloads : null
  } catch {
    return null
  }
}

async function fetchGithubStars(
  owner: string,
  repo: string
): Promise<number | null> {
  try {
    const response = await fetch(
      `https://api.github.com/repos/${owner}/${repo}`,
      {
        headers: {
          Accept: "application/vnd.github+json",
          "User-Agent": USER_AGENT,
        },
      }
    )

    if (!response.ok) return null

    const data = (await response.json()) as GithubRepoResponse
    const stars = data.stargazers_count

    return typeof stars === "number" ? stars : null
  } catch {
    return null
  }
}

export function formatStatCount(value: number): string {
  if (value >= 1_000_000) {
    return `${trimTrailingZero(value / 1_000_000)}M`
  }

  if (value >= 10_000) {
    return `${trimTrailingZero(value / 1_000)}k`
  }

  if (value >= 1_000) {
    return value.toLocaleString("en-US")
  }

  return String(value)
}

export function formatDownloads(value: number): string {
  return `${formatStatCount(value)} ${value === 1 ? "download" : "downloads"}`
}

export function formatStars(value: number): string {
  return `${formatStatCount(value)} ${value === 1 ? "star" : "stars"}`
}

function trimTrailingZero(value: number): string {
  return value.toFixed(1).replace(/\.0$/, "")
}
