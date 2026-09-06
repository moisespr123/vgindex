#!/usr/bin/env bash
set -Eeuo pipefail

phase="initialization"

on_error() {
    local exit_code=$?
    echo "ERROR: restore failed during ${phase} (exit code ${exit_code})" >&2
    exit "$exit_code"
}
trap on_error ERR

if [[ $# -ne 1 ]]; then
    echo "Usage: $0 <vgindex-backup-YYYYMMDDTHHMMSSZ.tar.gz>" >&2
    exit 2
fi

archive="$1"
if [[ ! -f "$archive" ]]; then
    echo "ERROR: backup archive not found: ${archive}" >&2
    exit 2
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
archive="$(realpath "$archive")"

cd "$repo_root"

phase="validating backup archive"
required_entries=(
    databases/app.dump
    databases/phpbb.dump
    databases/mediawiki.dump
    phpbb_files
    phpbb_avatars
    mediawiki_uploads
)
if ! tar -tzf "$archive" "${required_entries[@]}" >/dev/null; then
    echo "ERROR: backup archive is invalid or incomplete: ${archive}" >&2
    exit 2
fi

phase="deleting local Compose containers and volumes"
docker compose down --volumes --remove-orphans

phase="starting fresh local PostgreSQL"
docker compose up -d --wait postgres

phase="restoring databases and content volumes"
docker compose --profile backup run --rm --no-deps -T \
    -v "${archive}:/input/backup.tar.gz:ro" \
    restore

phase="refreshing local phpBB configuration"
docker compose run --rm --no-deps -T \
    -e PHPBB_BOOTSTRAP_MODE=force \
    phpbb true

phase="starting local services"
docker compose up -d app phpbb mediawiki caddy

trap - ERR
echo "Local restore complete: ${archive}"
