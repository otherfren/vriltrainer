#!/usr/bin/env bash
#
# A snapshot of the audit log, verified, compressed, kept — and, if configured, sent off the box.
#
# The database is not user data that could be re-collected. It is the record every past trial is
# checked against, so losing it does not lose a day's activity; it retroactively takes the
# verifiability away from every trial ever run. That is what this job exists for, and it is why
# every step here refuses rather than warns.
#
# Four things it does that a `cp` does not:
#
#   1. `VACUUM INTO` instead of copying the file. In WAL mode almost nothing lives in the main
#      file — on this box the live `.db` was 4 KB while its `-wal` held 3 MB. Copy the `.db` alone
#      and the backup is empty; copy the three files under a concurrent writer and they need not
#      agree. `VACUUM INTO` takes one read transaction and writes a single consistent file.
#   2. Walks the hash chain. A file that opens as SQLite is not yet an audit log.
#   3. Refuses a snapshot shorter than the last one. The log only ever grows, so a smaller count
#      means the tail is missing — and a truncated chain still verifies, because the first N
#      entries of a valid log are a valid log. Only the comparison catches it.
#   4. Reads back what it just wrote and verifies that, rather than trusting the write.
#
# The archives are plain gzip, not encrypted. Nothing in the schema is a secret: access tokens,
# handoff codes and admin keys are stored only as hashes, and the log deliberately carries the
# opaque account id instead of a name. The database is a record meant to be checkable by anyone,
# so the thing worth protecting is that it survives and still verifies, not that it stays unread.
#
# Everything is configured through `backup.env` beside it, so the copy in the repo and the copy on
# the box stay byte-identical — unlike the service unit, which had to diverge.
#
# Usage:  backup.sh            take, verify, compress, prune, upload
#         backup.sh --check    verify the newest archive and stop, changing nothing
#         backup.sh --restore <archive> <dest.db>    unpack and verify to a chosen path
set -euo pipefail

ROOT="${VRIL_ROOT:-/home/bob/vriltrainer}"
[ -f "$ROOT/backup.env" ] && . "$ROOT/backup.env"

DB="${VRIL_DB:-$ROOT/shared/vriltrainer.db}"
DEST="${VRIL_BACKUP_DIR:-$ROOT/backups}"
VERIFY="${VRIL_VERIFY_LOG:-$ROOT/shared/verify_log}"
INDEX="$DEST/INDEX"
STATE="$DEST/.last-count"

# Retention. Everything inside KEEP_ALL_DAYS is kept; past that one archive per day up to
# KEEP_DAILY_DAYS; past that one per month, forever. A year of trials is a few megabytes, so the
# tail costs nothing and the oldest record is the one hardest to reconstruct.
KEEP_ALL_DAYS="${VRIL_KEEP_ALL_DAYS:-7}"
KEEP_DAILY_DAYS="${VRIL_KEEP_DAILY_DAYS:-90}"

die() { echo "backup: $*" >&2; exit 1; }
note() { echo "backup: $*" >&2; }

preflight() {
    [ -f "$DB" ] || die "no database at $DB"
    [ -x "$VERIFY" ] || die "no verify_log at $VERIFY — build it with: cargo build --release --bin verify_log"
    command -v sqlite3 >/dev/null || die "sqlite3 not installed"
    mkdir -p "$DEST"
}

# Unpack an archive to $1 and walk its chain. Prints the entry count on stdout.
check_archive() {
    local archive="$1" out="$2" floor="${3:-0}"
    gunzip -c "$archive" > "$out" || die "$archive: could not decompress"
    [ "$(sqlite3 "$out" 'PRAGMA integrity_check;')" = "ok" ] || die "$archive: SQLite integrity check failed"
    "$VERIFY" --db "$out" --at-least "$floor"
}

newest_archive() {
    # ISO-8601 in the name, so lexical order is chronological.
    find "$DEST" -maxdepth 1 -name 'vriltrainer-*.db.gz' | sort | tail -1
}

cmd_check() {
    preflight
    local archive tmp count
    archive="$(newest_archive)"
    [ -n "$archive" ] || die "no archives in $DEST"
    tmp="$(mktemp -d)"; trap "rm -rf '$tmp'" EXIT
    count="$(check_archive "$archive" "$tmp/check.db" 0)"
    note "$(basename "$archive"): $count entries, unpacks and verifies"
}

cmd_restore() {
    preflight
    local archive="$1" dest="$2"
    [ -f "$archive" ] || die "no archive at $archive"
    [ -e "$dest" ] && die "$dest exists — restore refuses to overwrite"
    local count
    count="$(check_archive "$archive" "$dest" 0)" || { rm -f "$dest"; exit 1; }
    note "restored $count entries to $dest"
    note "the service applies migrations on open; stop both instances before pointing them here"
}

prune() {
    local now_s cutoff_all cutoff_daily
    now_s="$(date -u +%s)"
    cutoff_all=$(( now_s - KEEP_ALL_DAYS * 86400 ))
    cutoff_daily=$(( now_s - KEEP_DAILY_DAYS * 86400 ))

    local seen_day="" seen_month="" f base stamp when day month
    # Newest first, so the archive kept for a day or month is the last one of it — the one whose
    # chain is longest.
    while IFS= read -r f; do
        base="$(basename "$f")"
        stamp="${base#vriltrainer-}"; stamp="${stamp%%-n*}"        # 20260726T203500Z
        day="${stamp%T*}"                                          # 20260726
        month="${day:0:6}"                                         # 202607
        when="$(date -u -d "${day:0:4}-${day:4:2}-${day:6:2} ${stamp:9:2}:${stamp:11:2}:${stamp:13:2}" +%s 2>/dev/null)" || continue

        if   [ "$when" -ge "$cutoff_all" ];   then seen_day="$day"; seen_month="$month"; continue
        elif [ "$when" -ge "$cutoff_daily" ]; then
            if [ "$day" != "$seen_day" ]; then seen_day="$day"; seen_month="$month"; continue; fi
        else
            if [ "$month" != "$seen_month" ]; then seen_month="$month"; continue; fi
        fi
        rm -f "$f"
        note "pruned $base"
    done < <(find "$DEST" -maxdepth 1 -name 'vriltrainer-*.db.gz' | sort -r)
}

upload() {
    local file="$1" name="$2"
    if [ -z "${S3_ENDPOINT:-}" ] || [ -z "${S3_BUCKET:-}" ]; then
        note "no S3_ENDPOINT/S3_BUCKET in $ROOT/backup.env — local copy only"
        return 0
    fi
    # Any S3-compatible endpoint will do. The object is plain gzip and carries no secret, so the
    # store needs to be durable, not trusted. --aws-sigv4 is in curl since 7.75; no extra client
    # to keep installed.
    if curl -sSf --retry 3 --retry-delay 5 \
            --aws-sigv4 "aws:amz:${S3_REGION:-us-east-1}:s3" \
            --user "${S3_KEY_ID}:${S3_SECRET}" \
            -T "$file" "${S3_ENDPOINT%/}/${S3_BUCKET}/${name}" >/dev/null; then
        note "uploaded $name to ${S3_ENDPOINT%/}/${S3_BUCKET}"
    else
        # Loud, but not fatal: the local copy exists and is verified, and a failing endpoint must
        # not stop tomorrow's snapshot from being taken.
        note "WARNING upload of $name failed — local copy kept"
        return 1
    fi
}

cmd_backup() {
    preflight
    local tmp; tmp="$(mktemp -d)"; trap "rm -rf '$tmp'" EXIT

    sqlite3 "$DB" "VACUUM INTO '$tmp/snap.db'" || die "VACUUM INTO failed on $DB"
    [ "$(sqlite3 "$tmp/snap.db" 'PRAGMA integrity_check;')" = "ok" ] || die "snapshot fails SQLite integrity check"

    local floor count
    floor="$(cat "$STATE" 2>/dev/null || echo 0)"
    # On a copy: verify_log opens through Db::open, which applies migrations. The archived bytes
    # stay the ones VACUUM produced.
    cp "$tmp/snap.db" "$tmp/check.db"
    count="$("$VERIFY" --db "$tmp/check.db" --at-least "$floor")"

    local stamp name
    stamp="$(date -u +%Y%m%dT%H%M%SZ)"
    name="vriltrainer-${stamp}-n${count}.db.gz"
    gzip -9 < "$tmp/snap.db" > "$tmp/$name"
    chmod 644 "$tmp/$name"

    # The loop closed: read back what was just written rather than trusting the write.
    local back
    back="$(check_archive "$tmp/$name" "$tmp/back.db" "$count")"
    [ "$back" = "$count" ] || die "round trip lost entries: wrote $count, read back $back"

    mv "$tmp/$name" "$DEST/$name"
    echo "$count" > "$STATE"
    printf '%s\t%s\t%s\t%s\n' "$stamp" "$count" "$(sha256sum "$DEST/$name" | cut -d' ' -f1)" "$name" >> "$INDEX"
    note "$name: $count entries, verified through unpack"

    prune
    upload "$DEST/$name" "$name" || true
}

case "${1:-}" in
    "")         cmd_backup ;;
    --check)    cmd_check ;;
    --restore)  [ $# -eq 3 ] || die "usage: backup.sh --restore <archive> <dest.db>"; cmd_restore "$2" "$3" ;;
    *)          die "unknown argument: $1" ;;
esac
