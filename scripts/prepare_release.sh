#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NOTES_DIR="${ROOT_DIR}/dist/release-prep"
ALLOW_DIRTY=0
SKIP_VERIFY=0
VERSION_OVERRIDE=""

usage() {
    cat <<'EOF'
Usage: ./scripts/prepare_release.sh [--version <version>] [--skip-verify] [--allow-dirty]

Prepare the next release locally by:
- inferring the next version from commits since the latest tag
- updating Cargo.toml when needed
- generating release notes draft
- optionally running local verification
EOF
}

require_clean_worktree() {
    if [[ "${ALLOW_DIRTY}" -eq 1 ]]; then
        return
    fi

    if ! git diff --quiet || ! git diff --cached --quiet; then
        echo "Working tree has uncommitted tracked changes. Commit or stash them first, or use --allow-dirty." >&2
        exit 1
    fi
}

resolve_version() {
    sed -nE 's/^version = "(.*)"/\1/p' "${ROOT_DIR}/Cargo.toml" | head -n 1
}

resolve_latest_tag() {
    git describe --tags --match 'v*.*.*' --abbrev=0 2>/dev/null || true
}

split_semver() {
    local version="$1"
    IFS='.' read -r SEMVER_MAJOR SEMVER_MINOR SEMVER_PATCH <<< "${version}"
}

bump_semver() {
    local version="$1"
    local bump_kind="$2"

    split_semver "${version}"

    case "${bump_kind}" in
        major)
            if [[ "${SEMVER_MAJOR}" -eq 0 ]]; then
                echo "0.$((SEMVER_MINOR + 1)).0"
            else
                echo "$((SEMVER_MAJOR + 1)).0.0"
            fi
            ;;
        minor)
            echo "${SEMVER_MAJOR}.$((SEMVER_MINOR + 1)).0"
            ;;
        patch)
            echo "${SEMVER_MAJOR}.${SEMVER_MINOR}.$((SEMVER_PATCH + 1))"
            ;;
        *)
            echo "Unsupported bump kind: ${bump_kind}" >&2
            exit 1
            ;;
    esac
}

detect_bump_kind() {
    local latest_tag="$1"
    local range_args=()

    if [[ -n "${latest_tag}" ]]; then
        range_args+=("${latest_tag}..HEAD")
    fi

    local log_output
    log_output="$(git log --format='%s%n%b<<END>>' "${range_args[@]}")"

    if [[ -z "${log_output}" ]]; then
        echo "none"
        return
    fi

    if grep -Eq 'BREAKING CHANGE|^[a-z]+(\([^)]+\))?!:' <<< "${log_output}"; then
        echo "major"
        return
    fi

    if git log --format='%s' "${range_args[@]}" | grep -Eq '^feat(\([^)]+\))?: '; then
        echo "minor"
        return
    fi

    echo "patch"
}

update_cargo_version() {
    local next_version="$1"
    perl -0pi -e 's/^version = "\Q'"${2}"'\E"$/version = "'"${next_version}"'"/m' "${ROOT_DIR}/Cargo.toml"
}

collect_commits() {
    local latest_tag="$1"
    local format="$2"
    if [[ -n "${latest_tag}" ]]; then
        git log --reverse --format="${format}" "${latest_tag}..HEAD"
    else
        git log --reverse --format="${format}"
    fi
}

generate_release_notes() {
    local latest_tag="$1"
    local next_version="$2"
    local bump_kind="$3"
    local notes_file="${NOTES_DIR}/RELEASE_NOTES-v${next_version}.md"

    local feature_commits fix_commits refactor_commits misc_commits
    feature_commits="$(collect_commits "${latest_tag}" '%s' | grep -E '^feat(\([^)]+\))?: ' || true)"
    fix_commits="$(collect_commits "${latest_tag}" '%s' | grep -E '^fix(\([^)]+\))?: ' || true)"
    refactor_commits="$(collect_commits "${latest_tag}" '%s' | grep -E '^refactor(\([^)]+\))?: ' || true)"
    misc_commits="$(collect_commits "${latest_tag}" '%s' | grep -Ev '^(feat|fix|refactor)(\([^)]+\))?: ' || true)"

    mkdir -p "${NOTES_DIR}"

    {
        echo "# Robinne v${next_version} 发布说明草稿"
        echo
        if [[ -n "${latest_tag}" ]]; then
            echo "- 上一个标签：\`${latest_tag}\`"
        else
            echo "- 上一个标签：无，这是首个 release"
        fi
        echo "- 版本变更类型：\`${bump_kind}\`"
        echo
        echo "## 变更摘要"
        echo

        if [[ -n "${feature_commits}" ]]; then
            echo "### 新功能"
            while IFS= read -r line; do
                [[ -n "${line}" ]] && echo "- ${line}"
            done <<< "${feature_commits}"
            echo
        fi

        if [[ -n "${fix_commits}" ]]; then
            echo "### 修复"
            while IFS= read -r line; do
                [[ -n "${line}" ]] && echo "- ${line}"
            done <<< "${fix_commits}"
            echo
        fi

        if [[ -n "${refactor_commits}" ]]; then
            echo "### 重构与优化"
            while IFS= read -r line; do
                [[ -n "${line}" ]] && echo "- ${line}"
            done <<< "${refactor_commits}"
            echo
        fi

        if [[ -n "${misc_commits}" ]]; then
            echo "### 其他"
            while IFS= read -r line; do
                [[ -n "${line}" ]] && echo "- ${line}"
            done <<< "${misc_commits}"
            echo
        fi
    } > "${notes_file}"

    echo "${notes_file}"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --version)
            VERSION_OVERRIDE="${2:-}"
            shift 2
            ;;
        --skip-verify)
            SKIP_VERIFY=1
            shift
            ;;
        --allow-dirty)
            ALLOW_DIRTY=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

cd "${ROOT_DIR}"
require_clean_worktree

current_version="$(resolve_version)"
if [[ -z "${current_version}" ]]; then
    echo "Failed to resolve version from Cargo.toml" >&2
    exit 1
fi

latest_tag="$(resolve_latest_tag)"
latest_version="${latest_tag#v}"

if [[ -n "${VERSION_OVERRIDE}" ]]; then
    next_version="${VERSION_OVERRIDE}"
    bump_kind="manual"
elif [[ -z "${latest_tag}" ]]; then
    next_version="${current_version}"
    bump_kind="initial"
else
    commit_count="$(git rev-list --count "${latest_tag}..HEAD")"
    if [[ "${commit_count}" -eq 0 ]]; then
        echo "No commits found since ${latest_tag}. Nothing to release." >&2
        exit 1
    fi

    bump_kind="$(detect_bump_kind "${latest_tag}")"
    if [[ "${bump_kind}" == "none" ]]; then
        echo "Unable to determine version bump from commits since ${latest_tag}." >&2
        exit 1
    fi

    next_version="$(bump_semver "${latest_version}" "${bump_kind}")"
fi

if [[ "${current_version}" != "${next_version}" ]]; then
    update_cargo_version "${next_version}" "${current_version}"
fi

notes_file="$(generate_release_notes "${latest_tag}" "${next_version}" "${bump_kind}")"

if [[ "${SKIP_VERIFY}" -eq 0 ]]; then
    cargo test --quiet
    cargo build --release

    if [[ "$(uname -s)" == "Darwin" ]]; then
        ./build_mac_app.sh --version "${next_version}" --output-dir "${NOTES_DIR}" --skip-build
    fi
fi

echo "Prepared release version: ${next_version}"
echo "Suggested tag: v${next_version}"
echo "Release notes draft: ${notes_file}"
if [[ "${SKIP_VERIFY}" -eq 0 && "$(uname -s)" == "Darwin" ]]; then
    echo "macOS package: ${NOTES_DIR}/Robinne-v${next_version}-macos.zip"
fi
