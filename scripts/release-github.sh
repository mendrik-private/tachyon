#!/bin/sh
# Create and push the immutable version tag that starts the release workflow.
set -eu

usage() {
    printf 'Usage: %s [--detach]\n' "$0" >&2
    printf 'Create and push the version tag declared by markdown-app.\n' >&2
}

die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
script_path=$project_root/scripts/release-github.sh

case "${1-}" in
    --detach)
        if [ "$#" -ne 1 ]; then
            usage
            exit 2
        fi

        release_dir=$project_root/target/release
        run_id=$(date -u +%Y%m%dT%H%M%SZ)-$$
        log_path=$release_dir/github-release-$run_id.log
        pid_path=$release_dir/github-release-$run_id.pid
        mkdir -p "$release_dir"
        umask 077
        nohup "$script_path" >"$log_path" 2>&1 < /dev/null &
        pid=$!
        printf '%s\n' "$pid" >"$pid_path"
        printf 'Started GitHub release launcher (PID %s).\n' "$pid"
        printf 'Log: %s\nPID file: %s\n' "$log_path" "$pid_path"
        exit 0
        ;;
    --help|-h)
        usage
        exit 0
        ;;
    '')
        ;;
    *)
        usage
        exit 2
        ;;
esac

cd "$project_root"

for command in git gh python3; do
    command -v "$command" >/dev/null 2>&1 || \
        die "Required command is unavailable: $command"
done

if [ -n "$(git status --porcelain)" ]; then
    die 'Working tree is not clean; commit, stash, or discard changes before creating a release tag.'
fi

branch=$(git branch --show-current)
if [ "$branch" != main ]; then
    die "Release tags must be created from main, not ${branch:-a detached HEAD}."
fi

package_version=$(scripts/verify-release-metadata.py "v$(
    python3 -c \
        'import pathlib,tomllib; print(tomllib.loads(pathlib.Path("crates/markdown-app/Cargo.toml").read_text())["package"]["version"])'
)")
release_tag=v$package_version

if ! gh auth status --hostname github.com >/dev/null 2>&1; then
    die 'GitHub CLI authentication is unavailable or invalid. Run: gh auth login --hostname github.com'
fi

repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner)
head_sha=$(git rev-parse HEAD)
git fetch --quiet origin main
if ! git merge-base --is-ancestor "$head_sha" origin/main; then
    die 'HEAD is not an ancestor of origin/main; push main before creating a release tag.'
fi

deployments=$(gh api --method GET "repos/$repo/deployments" \
    -f sha="$head_sha" -f environment=github-pages -f per_page=100 \
    --jq 'sort_by(.created_at) | reverse | .[].id')
if [ -z "$deployments" ]; then
    die "No github-pages deployment exists for $head_sha; wait for the Documentation workflow on main."
fi

successful_deployment=
successful_status=
for deployment in $deployments; do
    status=$(gh api "repos/$repo/deployments/$deployment/statuses" \
        --jq '(map(select(.state == "success")) | sort_by(.created_at) | reverse | .[0] // empty) | [.environment_url // .target_url // "", .created_at] | @tsv')
    if [ -n "$status" ]; then
        successful_deployment=$deployment
        successful_status=$status
        break
    fi
done
if [ -z "$successful_deployment" ]; then
    die "No successful github-pages deployment exists for $head_sha; wait for the Documentation workflow on main."
fi

if git show-ref --tags --verify --quiet "refs/tags/$release_tag"; then
    die "Local tag $release_tag already exists; refusing to move or reuse it."
fi
if git ls-remote --exit-code --tags origin "refs/tags/$release_tag" >/dev/null 2>&1; then
    die "Remote tag $release_tag already exists; refusing to overwrite it."
fi
if gh release view "$release_tag" >/dev/null 2>&1; then
    die "GitHub release $release_tag already exists; refusing to reuse it."
fi

printf 'Verified github-pages deployment %s for %s: %s\n' \
    "$successful_deployment" "$head_sha" "$successful_status"
git tag -a "$release_tag" -m "Release $release_tag"
if ! git push origin "refs/tags/$release_tag"; then
    die "Pushing $release_tag failed. The newly-created local tag remains for inspection."
fi

printf 'Pushed %s. GitHub Actions is checking, packaging, and publishing the release.\n' \
    "$release_tag"
printf 'Follow it at: https://github.com/%s/actions/workflows/release.yml\n' "$repo"
