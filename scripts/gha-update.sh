#!/usr/bin/env bash
# Update GitHub Actions SHA pins to the latest release within the same major version.
#
# Scans .github/workflows/*.yml for pinned actions in the format:
#   uses: owner/repo@SHA # vTAG from DD.MM.YYYY
# and updates them to the latest release matching the same major version.
#
# Usage:
#   gha-update          # dry-run: shows what would change
#   gha-update --apply  # writes updates to workflow files

set -euo pipefail

WORKFLOW_DIR=".github/workflows"
APPLY=false

if [[ "${1:-}" == "--apply" ]]; then
  APPLY=true
fi

if [[ ! -d "$WORKFLOW_DIR" ]]; then
  echo "Error: $WORKFLOW_DIR not found. Run from the repo root." >&2
  exit 1
fi

if ! command -v gh &>/dev/null; then
  echo "Error: gh CLI not found. Install it first." >&2
  exit 1
fi

# Collect unique owner/repo → current tag mappings from all workflow files
declare -A SEEN_REPOS  # owner/repo → major_prefix (e.g. "v4")
declare -A CURRENT_TAGS  # owner/repo → current full tag
declare -A CURRENT_SHAS  # owner/repo → current sha

while IFS= read -r line; do
  # Match: uses: owner/repo@HEX_SHA # vTAG from DD.MM.YYYY
  if [[ "$line" =~ uses:\ ([a-zA-Z0-9._-]+/[a-zA-Z0-9._-]+)@([0-9a-f]{40})\ +#\ +(v[^ ]+)\ +from\ +[0-9]{2}\.[0-9]{2}\.[0-9]{4} ]]; then
    repo="${BASH_REMATCH[1]}"
    sha="${BASH_REMATCH[2]}"
    tag="${BASH_REMATCH[3]}"

    if [[ -z "${SEEN_REPOS[$repo]:-}" ]]; then
      # Extract major version prefix: v4.2.1 → v4, v1.6.0 → v1
      major="${tag%%.*}"
      # Handle tags without dots (e.g. v4 → v4)
      if [[ "$major" == "$tag" ]]; then
        major="$tag"
      fi
      SEEN_REPOS[$repo]="$major"
      CURRENT_TAGS[$repo]="$tag"
      CURRENT_SHAS[$repo]="$sha"
    fi
  fi
done < <(cat "$WORKFLOW_DIR"/*.yml)

if [[ ${#SEEN_REPOS[@]} -eq 0 ]]; then
  echo "No pinned actions found in $WORKFLOW_DIR/*.yml"
  exit 0
fi

echo "Found ${#SEEN_REPOS[@]} pinned action(s):"
echo ""

HAS_UPDATES=false

for repo in $(printf '%s\n' "${!SEEN_REPOS[@]}" | sort); do
  major="${SEEN_REPOS[$repo]}"
  current_tag="${CURRENT_TAGS[$repo]}"
  current_sha="${CURRENT_SHAS[$repo]}"

  echo "── $repo (pinned: $current_tag, major: $major)"

  # Fetch latest release matching the major version
  latest_tag=""
  latest_tag=$(gh api "repos/$repo/releases" \
    --paginate \
    --jq "[.[] | select(.draft == false and .prerelease == false) | select(.tag_name | startswith(\"${major}\"))] | sort_by(.published_at) | reverse | .[0].tag_name // empty" 2>/dev/null) || true

  if [[ -z "$latest_tag" ]]; then
    # Fallback: try git tags via the API (some repos don't use GitHub Releases)
    latest_tag=$(gh api "repos/$repo/git/matching-refs/tags/${major}" \
      --jq "[.[].ref | ltrimstr(\"refs/tags/\")] | sort | reverse | .[0] // empty" 2>/dev/null) || true
  fi

  if [[ -z "$latest_tag" ]]; then
    echo "   ⚠ No release found for major $major — skipping"
    echo ""
    continue
  fi

  # Resolve tag to commit SHA (dereference annotated tags)
  tag_type=$(gh api "repos/$repo/git/ref/tags/$latest_tag" --jq '.object.type' 2>/dev/null) || true
  tag_sha=$(gh api "repos/$repo/git/ref/tags/$latest_tag" --jq '.object.sha' 2>/dev/null) || true

  if [[ -z "$tag_sha" ]]; then
    echo "   ⚠ Could not resolve tag $latest_tag — skipping"
    echo ""
    continue
  fi

  if [[ "$tag_type" == "tag" ]]; then
    # Annotated tag — dereference to get the commit SHA
    tag_sha=$(gh api "repos/$repo/git/tags/$tag_sha" --jq '.object.sha' 2>/dev/null) || true
  fi

  if [[ -z "$tag_sha" ]]; then
    echo "   ⚠ Could not resolve SHA for $latest_tag — skipping"
    echo ""
    continue
  fi

  today=$(date +%d.%m.%Y)

  if [[ "$tag_sha" == "$current_sha" ]]; then
    echo "   ✓ Already up to date ($latest_tag)"
  else
    HAS_UPDATES=true
    echo "   Current: $current_tag @ ${current_sha:0:12}…"
    echo "   Latest:  $latest_tag @ ${tag_sha:0:12}…"

    if $APPLY; then
      # Build sed replacement: match this repo's old SHA+comment, replace with new
      for f in "$WORKFLOW_DIR"/*.yml; do
        sed -i "s|${repo}@${current_sha} *# *${current_tag} from [0-9]\{2\}\.[0-9]\{2\}\.[0-9]\{4\}|${repo}@${tag_sha} # ${latest_tag} from ${today}|g" "$f"
      done
      echo "   ✏ Updated in workflow files"
    else
      echo "   → Run with --apply to update"
    fi
  fi
  echo ""
done

if $APPLY && $HAS_UPDATES; then
  echo "Done. Review changes with: git diff"
elif ! $HAS_UPDATES; then
  echo "All actions are up to date."
else
  echo "Dry run complete. Run 'gha-update --apply' to apply changes."
fi
