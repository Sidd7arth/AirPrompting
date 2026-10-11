/**
 * GitHub Releases resolution.
 *
 * The download buttons must never be broken: either the API returns a release
 * with a matching asset, or the page falls back to explicit platform choices
 * that link at the releases index. Asset names are discovered from release
 * metadata — nothing about the bundler's file naming is hardcoded beyond the
 * patterns below.
 */

import { latestReleaseUrl, releasesIndexUrl } from "./config";

/** What a visitor can be offered, once resolution has happened. */
export interface ResolvedDownload {
  /** Platform this artifact is for. */
  platform: "macos" | "windows";
  /** Direct URL to the asset binary. */
  url: string;
  /** Asset file name, shown in the confirmation dialog. */
  fileName: string;
  /** The release this asset came from (e.g. "v0.1.0"). */
  releaseTag: string;
  /** True when the release is marked prerelease — surfaced honestly in the UI. */
  prerelease: boolean;
}

/** No usable release exists. The honest state, and today the initial state. */
export interface NoRelease {
  kind: "none";
}

export type Resolution = { kind: "resolved"; downloads: ResolvedDownload[] } | NoRelease;

/**
 * Asset-name patterns, in preference order for each platform.
 *
 * - macOS: `.dmg` wins over a bare `.app` tarball; arm64 over x64 when both
 *   exist, but both are offered in the fallback list if only one is present.
 * - Windows: NSIS `.exe` (user-friendly) wins over `.msi` (enterprise).
 */
const PATTERNS: { platform: "macos" | "windows"; re: RegExp }[] = [
  { platform: "macos", re: /\.dmg$/i },
  { platform: "windows", re: /\.exe$/i },
  { platform: "windows", re: /\.msi$/i },
  { platform: "macos", re: /\.app\.tar\.gz$/i },
];

interface GhAsset {
  name?: unknown;
  browser_download_url?: unknown;
}

interface GhRelease {
  tag_name?: unknown;
  prerelease?: unknown;
  draft?: unknown;
  assets?: unknown;
}

function isGhRelease(value: unknown): value is GhRelease {
  return typeof value === "object" && value !== null;
}

/**
 * Resolve the latest stable release into per-platform downloads.
 *
 * Never throws: any failure — network, rate limit, 404 with no releases yet,
 * a release whose assets match nothing — resolves to `{ kind: "none" }` and
 * the page shows its intentional fallback state.
 */
export async function resolveLatestRelease(fetchImpl: typeof fetch = fetch): Promise<Resolution> {
  let payload: unknown;
  try {
    const response = await fetchImpl(latestReleaseUrl(), {
      headers: { Accept: "application/vnd.github+json" },
    });
    if (!response.ok) return { kind: "none" };
    payload = await response.json();
  } catch {
    return { kind: "none" };
  }
  if (!isGhRelease(payload)) return { kind: "none" };
  if (payload.draft === true) return { kind: "none" };
  if (!Array.isArray(payload.assets)) return { kind: "none" };

  const tag = typeof payload.tag_name === "string" ? payload.tag_name : "?";
  const prerelease = payload.prerelease === true;

  const downloads: ResolvedDownload[] = [];
  for (const { platform, re } of PATTERNS) {
    for (const asset of payload.assets as GhAsset[]) {
      const name = typeof asset.name === "string" ? asset.name : "";
      const url =
        typeof asset.browser_download_url === "string" ? asset.browser_download_url : "";
      if (!name || !url || !re.test(name)) continue;
      if (downloads.some((d) => d.platform === platform)) continue; // first pattern wins
      downloads.push({ platform, url, fileName: name, releaseTag: tag, prerelease });
      break;
    }
  }

  return downloads.length > 0 ? { kind: "resolved", downloads } : { kind: "none" };
}

/** Where "view all releases" points — also the fallback href for both platforms. */
export function fallbackUrl(): string {
  return releasesIndexUrl();
}
