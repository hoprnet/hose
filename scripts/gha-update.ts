#!/usr/bin/env -S deno run --allow-read=.github/workflows --allow-write=.github/workflows --allow-net=api.github.com --allow-env=GITHUB_TOKEN,PATH --allow-run=gh
// Update GitHub Actions SHA pins to the latest release.
//
// Scans .github/workflows/*.yml for pinned actions in the format:
//   uses: owner/repo@SHA # vTAG from DD.MM.YYYY
// and updates them to the latest release. By default, stays within the same
// major version. Use --major to search across major versions.
//
// Usage:
//   gha-update                  # dry-run: shows what would change
//   gha-update --apply          # writes updates to workflow files
//   gha-update --major          # dry-run across major versions
//   gha-update --major --apply  # update across major versions
//   gha-update -h | --help      # show help

import { parseArgs } from "jsr:@std/cli/parse-args";

const WORKFLOW_DIR = ".github/workflows";

// --- Types ---

interface PinnedAction {
  repo: string;
  sha: string;
  tag: string;
  major: string;
}

interface GitHubRelease {
  tag_name: string;
  draft: boolean;
  prerelease: boolean;
  published_at: string;
}

interface GitHubRef {
  ref: string;
  object: { type: string; sha: string };
}

interface GitHubTag {
  object: { type: string; sha: string };
}

// --- Auth ---

async function getToken(): Promise<string> {
  const envToken = Deno.env.get("GITHUB_TOKEN");
  if (envToken) return envToken;

  const proc = new Deno.Command("gh", {
    args: ["auth", "token"],
    stdout: "piped",
    stderr: "piped",
    clearEnv: true,
    env: { PATH: Deno.env.get("PATH") ?? "" },
  });
  const { code, stdout } = await proc.output();
  if (code !== 0) {
    console.error(
      "Error: No GITHUB_TOKEN and `gh auth token` failed. Authenticate with `gh auth login`.",
    );
    Deno.exit(1);
  }
  return new TextDecoder().decode(stdout).trim();
}

async function ghFetch<T>(token: string, path: string): Promise<T | null> {
  const resp = await fetch(`https://api.github.com/${path}`, {
    headers: {
      Authorization: `Bearer ${token}`,
      Accept: "application/vnd.github+json",
    },
  });
  if (!resp.ok) return null;
  return (await resp.json()) as T;
}

// --- Pagination helper for releases ---

async function fetchAllReleases(
  token: string,
  repo: string,
): Promise<GitHubRelease[]> {
  const releases: GitHubRelease[] = [];
  let page = 1;
  while (true) {
    const resp = await fetch(
      `https://api.github.com/repos/${repo}/releases?per_page=100&page=${page}`,
      {
        headers: {
          Authorization: `Bearer ${token}`,
          Accept: "application/vnd.github+json",
        },
      },
    );
    if (!resp.ok) return releases;
    const batch = (await resp.json()) as GitHubRelease[];
    if (batch.length === 0) break;
    releases.push(...batch);
    // Stop early if we got less than a full page
    if (batch.length < 100) break;
    page++;
  }
  return releases;
}

// --- Parsing ---

const PIN_RE =
  /uses:\s+([a-zA-Z0-9._-]+\/[a-zA-Z0-9._-]+)@([0-9a-f]{40})\s+#\s+(v\S+)\s+from\s+\d{2}\.\d{2}\.\d{4}/g;

function parsePinnedActions(content: string): PinnedAction[] {
  const seen = new Map<string, PinnedAction>();
  for (const m of content.matchAll(PIN_RE)) {
    const [, repo, sha, tag] = m;
    if (seen.has(repo)) continue;
    // Extract major: v4.2.1 → v4, v15 → v15
    const major = tag.includes(".") ? tag.slice(0, tag.indexOf(".")) : tag;
    seen.set(repo, { repo, sha, tag, major });
  }
  return [...seen.values()];
}

// --- Resolution ---

async function findLatestTag(
  token: string,
  repo: string,
  major: string,
): Promise<string | null> {
  // Try releases first
  const releases = await fetchAllReleases(token, repo);
  const matching = releases
    .filter((r) => !r.draft && !r.prerelease && r.tag_name.startsWith(major))
    .sort((a, b) => b.published_at.localeCompare(a.published_at));

  if (matching.length > 0) return matching[0].tag_name;

  // Fallback: git matching-refs
  const refs = await ghFetch<GitHubRef[]>(
    token,
    `repos/${repo}/git/matching-refs/tags/${major}`,
  );
  if (!refs || refs.length === 0) return null;

  const tags = refs.map((r) => r.ref.replace("refs/tags/", "")).sort();
  return tags[tags.length - 1];
}

async function resolveTagSha(
  token: string,
  repo: string,
  tag: string,
): Promise<string | null> {
  const ref = await ghFetch<GitHubRef>(
    token,
    `repos/${repo}/git/ref/tags/${tag}`,
  );
  if (!ref) return null;

  // Dereference annotated tags
  if (ref.object.type === "tag") {
    const tagObj = await ghFetch<GitHubTag>(
      token,
      `repos/${repo}/git/tags/${ref.object.sha}`,
    );
    return tagObj?.object.sha ?? null;
  }

  return ref.object.sha;
}

// --- Main ---

async function main() {
  const flags = parseArgs(Deno.args, {
    boolean: ["apply", "major", "help"],
    alias: { h: "help" },
  });

  if (flags.help) {
    console.log(`gha-update — Update GitHub Actions SHA pins to latest releases

Scans .github/workflows/*.yml for pinned actions in the format:
  uses: owner/repo@SHA # vTAG from DD.MM.YYYY

Options:
  --apply        Write updates to workflow files (default: dry-run)
  --major        Search across major versions, not just the current one
  -h, --help     Show this help message`);
    Deno.exit(0);
  }

  const { apply, major } = flags;

  // Read all workflow files
  const files: { path: string; content: string }[] = [];
  for await (const entry of Deno.readDir(WORKFLOW_DIR)) {
    if (entry.isFile && entry.name.endsWith(".yml")) {
      const path = `${WORKFLOW_DIR}/${entry.name}`;
      files.push({ path, content: await Deno.readTextFile(path) });
    }
  }

  if (files.length === 0) {
    console.error(
      `Error: No .yml files in ${WORKFLOW_DIR}. Run from the repo root.`,
    );
    Deno.exit(1);
  }

  // Parse all pinned actions across all files
  const allContent = files.map((f) => f.content).join("\n");
  const actions = parsePinnedActions(allContent);

  if (actions.length === 0) {
    console.log(`No pinned actions found in ${WORKFLOW_DIR}/*.yml`);
    Deno.exit(0);
  }

  console.log(`Found ${actions.length} pinned action(s):\n`);

  const token = await getToken();
  let hasUpdates = false;

  for (const action of actions.sort((a, b) => a.repo.localeCompare(b.repo))) {
    const searchPrefix = major ? "v" : action.major;
    console.log(
      `── ${action.repo} (pinned: ${action.tag}, searching: ${searchPrefix}*)`,
    );

    const latestTag = await findLatestTag(token, action.repo, searchPrefix);
    if (!latestTag) {
      console.log(
        `   ⚠ No release found matching ${searchPrefix}* — skipping\n`,
      );
      continue;
    }

    const latestSha = await resolveTagSha(token, action.repo, latestTag);
    if (!latestSha) {
      console.log(`   ⚠ Could not resolve SHA for ${latestTag} — skipping\n`);
      continue;
    }

    if (latestSha === action.sha) {
      console.log(`   ✓ Already up to date (${latestTag})\n`);
      continue;
    }

    hasUpdates = true;
    const latestMajor = latestTag.includes(".")
      ? latestTag.slice(0, latestTag.indexOf("."))
      : latestTag;
    const isMajorBump = latestMajor !== action.major;
    console.log(`   Current: ${action.tag} @ ${action.sha.slice(0, 12)}…`);
    console.log(`   Latest:  ${latestTag} @ ${latestSha.slice(0, 12)}…`);
    if (isMajorBump) {
      console.log(
        `   ⚠ MAJOR version change: ${action.major} → ${latestMajor}`,
      );
    }

    if (apply) {
      const today = new Date();
      const dd = String(today.getDate()).padStart(2, "0");
      const mm = String(today.getMonth() + 1).padStart(2, "0");
      const yyyy = today.getFullYear();
      const dateStr = `${dd}.${mm}.${yyyy}`;

      const oldPattern = new RegExp(
        `${escapeRegex(action.repo)}@${action.sha}\\s+#\\s+${escapeRegex(
          action.tag,
        )}\\s+from\\s+\\d{2}\\.\\d{2}\\.\\d{4}`,
        "g",
      );
      const replacement = `${action.repo}@${latestSha} # ${latestTag} from ${dateStr}`;

      for (const file of files) {
        const updated = file.content.replace(oldPattern, replacement);
        if (updated !== file.content) {
          file.content = updated;
          await Deno.writeTextFile(file.path, updated);
        }
      }
      console.log(`   ✏ Updated in workflow files`);
    } else {
      console.log(`   → Run with --apply to update`);
    }
    console.log();
  }

  if (apply && hasUpdates) {
    console.log("Done. Review changes with: git diff");
  } else if (!hasUpdates) {
    console.log("All actions are up to date.");
  } else {
    console.log("Dry run complete. Run 'gha-update --apply' to apply changes.");
  }
}

function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

main();
