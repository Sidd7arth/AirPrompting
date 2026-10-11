/**
 * The public landing page (spec §115).
 *
 * A single job: get the right installer onto the visitor's machine, honestly.
 * No telemetry, no fake download counts, no invented claims — the copy states
 * what the product does and nothing more.
 *
 * Structure:
 *   config.ts   — RELEASE_CONFIG: where releases live (the only edit point)
 *   release.ts  — GitHub Releases resolution with a no-release fallback
 *   detect.ts   — OS detection from the user agent
 *   main.ts     — the page itself: hero, CTA, download confirmation
 */

/**
 * Where installers come from.
 *
 * `owner`/`repo` identify the GitHub repository whose Releases hold the
 * artifacts. When the project moves, this is the only file to edit — asset
 * names are discovered from release metadata, never hardcoded, so a renamed
 * bundler output keeps working.
 */
export const RELEASE_CONFIG = {
  owner: "airprompt",
  repo: "airprompt",
} as const;

/** The GitHub API endpoint returning the latest stable (non-draft, non-prerelease) release. */
export function latestReleaseUrl(): string {
  return `https://api.github.com/repos/${RELEASE_CONFIG.owner}/${RELEASE_CONFIG.repo}/releases/latest`;
}

/** Human-facing fallback: the full releases list. */
export function releasesIndexUrl(): string {
  return `https://github.com/${RELEASE_CONFIG.owner}/${RELEASE_CONFIG.repo}/releases`;
}
