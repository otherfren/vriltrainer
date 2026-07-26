#!/usr/bin/env bash
#
# A snapshot of the audit log, exported as plain JSON, verified, kept — and, if configured, sent
# off the box.
#
# The database is not user data that could be re-collected. It is the record every past trial is
# checked against, so losing it does not lose a day's activity; it retroactively takes the
# verifiability away from every trial ever run. That is what this job exists for, and it is why
# every step here refuses rather than warns.
#
# The archive is JSON, not a copy of the .db, and that is the point. A backup of a record whose
# whole claim is "anyone can check this" should not need SQLite — or any particular version of it,
# or an intact page format, or luck — to be read again. A text file can be opened by anything,
# diffed between two days, grepped for a trial id, and inspected without trusting the tool that
# opens it. The document carries its own schema, so it also says what it means, not only what it
# holds. A partially damaged binary page file is usually a total loss; a partially damaged text
# file is usually a small one.
#
# Five things it does that a `cp` — or a `.dump` — does not:
#
#   1. `VACUUM INTO` before reading anything. In WAL mode almost nothing lives in the main file —
#      on this box the live `.db` was 4 KB while its `-wal` held 3 MB. Read the `.db` alone and the
#      export is empty; read the three files under a concurrent writer and they need not agree.
#      `VACUUM INTO` takes one read transaction and writes a single consistent file, which is then
#      the only thing the export sees.
#   2. Walks the hash chain. A file that parses as JSON is not yet an audit log.
#   3. Refuses a snapshot shorter than the last one. The log only ever grows, so a smaller count
#      means the tail is missing — and a truncated chain still verifies, because the first N
#      entries of a valid log are a valid log. Only the comparison catches it.
#   4. Rebuilds a database from the JSON it just wrote and verifies *that*, rather than trusting
#      the export. An export nobody has ever imported is a guess.
#   5. Compares the rebuilt database against the snapshot with `.sha3sum`, which hashes the
#      content of every table. The chain walk only covers `log_entry`; this covers the rest —
#      accounts, stats, pool rows — column by column.
#
# The archive is plain text and not encrypted. Nothing in the schema is a secret: access tokens,
# handoff codes and admin keys are stored only as hashes, and the log deliberately carries the
# opaque account id instead of a name. The database is a record meant to be checkable by anyone,
# so the thing worth protecting is that it survives and still verifies, not that it stays unread.
# It is not published either — pending and rejected names are in there, and so is the mapping from
# an account to its attempts.
#
# Everything is configured through `backup.env` beside it, so the copy in the repo and the copy on
# the box stay byte-identical — unlike the service unit, which had to diverge.
#
# Usage:  backup.sh            take, verify, export, prune, upload
#         backup.sh --check    verify the newest archive and stop, changing nothing
#         backup.sh --restore <archive.json> <dest.db>    rebuild and verify to a chosen path
set -euo pipefail

ROOT="${VRIL_ROOT:-/home/bob/vriltrainer}"
[ -f "$ROOT/backup.env" ] && . "$ROOT/backup.env"

DB="${VRIL_DB:-$ROOT/shared/vriltrainer.db}"
DEST="${VRIL_BACKUP_DIR:-$ROOT/backups}"
VERIFY="${VRIL_VERIFY_LOG:-$ROOT/shared/verify_log}"
INDEX="$DEST/INDEX"
STATE="$DEST/.last-count"
FORMAT="vriltrainer-export-1"

# Retention. Everything inside KEEP_ALL_DAYS is kept; past that one archive per day up to
# KEEP_DAILY_DAYS; past that one per month, forever. The oldest record is the one hardest to
# reconstruct, so the long tail is kept even though text costs more than the packed pages did.
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

# Paths reach SQLite inside single-quoted string literals, and nothing here should ever contain a
# quote. Refuse rather than build a broken statement out of one.
safe_path() {
    case "$1" in *"'"*) die "path contains a single quote, which cannot be passed to sqlite3: $1" ;; esac
}

# --- the export ---------------------------------------------------------------------------------
#
# One JSON document: the DDL of every table, index and trigger, then every row of every table with
# its column names beside it. Rows come out one per line, so `grep` finds a trial id and a `diff`
# between two days shows exactly what was appended.
#
# `.mode json` is what writes the rows, so quoting, nulls and the full precision of the REAL
# columns are SQLite's problem rather than this script's.
export_json() {
    local db="$1" out="$2" count="$3"
    local tables t rows first=1
    safe_path "$out"

    ddl() {
        sqlite3 "$db" "SELECT json_group_array(sql) FROM (
                           SELECT sql FROM sqlite_master
                            WHERE type = '$1' AND sql IS NOT NULL AND name NOT LIKE 'sqlite_%'
                            ORDER BY name);"
    }

    tables="$(sqlite3 "$db" "SELECT name FROM sqlite_master
                              WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name;")"
    [ -n "$tables" ] || die "snapshot has no tables"

    {
        printf '{\n'
        printf '  "format": "%s",\n' "$FORMAT"
        printf '  "taken_at": "%s",\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        printf '  "log_entries": %s,\n' "$count"
        printf '  "schema": {\n'
        printf '    "tables": %s,\n'   "$(ddl table)"
        printf '    "indexes": %s,\n'  "$(ddl index)"
        printf '    "triggers": %s\n'  "$(ddl trigger)"
        printf '  },\n'
        printf '  "tables": {\n'
        for t in $tables; do
            [ "$first" -eq 1 ] || printf ',\n'
            first=0
            # An empty table prints nothing at all in json mode, which would be a syntax error.
            rows="$(sqlite3 "$db" -cmd ".mode json" "SELECT * FROM \"$t\" ORDER BY rowid;")"
            [ -n "$rows" ] || rows='[]'
            printf '    "%s": %s' "$t" "$rows"
        done
        printf '\n  }\n}\n'
    } > "$out"

    [ "$(sqlite3 :memory: "SELECT json_valid(readfile('$out'));")" = "1" ] \
        || die "the export is not valid JSON — refusing to keep it"
}

# --- the import ---------------------------------------------------------------------------------
#
# The reverse, and the reason the export can be trusted: schema, then rows, then indexes and
# triggers. That order matters. The pool-binding trigger refuses a commit row without a manifest
# hash once any commit row has one, which is right for a live append and wrong for a replay of
# rows that were already accepted, so triggers go on after the data is in.
#
# Columns are read back by name out of `pragma_table_info`, so a schema change needs no edit here.
import_json() {
    local json="$1" db="$2" t cols exprs
    safe_path "$json"; safe_path "$db"
    [ -f "$json" ] || die "no archive at $json"
    [ -e "$db" ] && die "$db exists — refusing to overwrite"

    [ "$(sqlite3 :memory: "SELECT json_valid(readfile('$json'));")" = "1" ] \
        || die "$json is not valid JSON"
    [ "$(sqlite3 :memory: "SELECT json_extract(readfile('$json'), '\$.format');")" = "$FORMAT" ] \
        || die "$json is not a $FORMAT document"

    apply_ddl() {
        sqlite3 :memory: "SELECT value || ';' FROM json_each(readfile('$json'), '\$.schema.$1');" \
            | sqlite3 "$db"
    }

    apply_ddl tables
    for t in $(sqlite3 :memory: "SELECT key FROM json_each(readfile('$json'), '\$.tables');"); do
        cols="$(sqlite3 "$db" "SELECT group_concat(q, ', ') FROM (
                                   SELECT '\"' || name || '\"' AS q
                                     FROM pragma_table_info('$t') ORDER BY cid);")"
        exprs="$(sqlite3 "$db" "SELECT group_concat(q, ', ') FROM (
                                    SELECT 'json_extract(value, ''\$.\"' || name || '\"'')' AS q
                                      FROM pragma_table_info('$t') ORDER BY cid);")"
        [ -n "$cols" ] || die "$json holds rows for a table its schema does not define: $t"
        sqlite3 "$db" "INSERT INTO \"$t\" ($cols)
                       SELECT $exprs FROM json_each(readfile('$json'), '\$.tables.\"$t\"');"
    done
    apply_ddl indexes
    apply_ddl triggers

    [ "$(sqlite3 "$db" 'PRAGMA integrity_check;')" = "ok" ] \
        || die "the database rebuilt from $json fails SQLite's integrity check"
}

# Rebuild an archive to $2 and walk its chain. Prints the entry count on stdout.
check_archive() {
    local archive="$1" out="$2" floor="${3:-0}"
    import_json "$archive" "$out"
    "$VERIFY" --db "$out" --at-least "$floor"
}

newest_archive() {
    # ISO-8601 in the name, so lexical order is chronological.
    find "$DEST" -maxdepth 1 -name 'vriltrainer-*.json' | sort | tail -1
}

cmd_check() {
    preflight
    local archive tmp count
    archive="$(newest_archive)"
    [ -n "$archive" ] || die "no archives in $DEST"
    tmp="$(mktemp -d)"; trap "rm -rf '$tmp'" EXIT
    count="$(check_archive "$archive" "$tmp/check.db" 0)"
    note "$(basename "$archive"): $count entries, rebuilds and verifies"
}

cmd_restore() {
    preflight
    local archive="$1" dest="$2" count
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
    done < <(find "$DEST" -maxdepth 1 -name 'vriltrainer-*.json' | sort -r)
}

upload() {
    local file="$1" name="$2"
    if [ -z "${S3_ENDPOINT:-}" ] || [ -z "${S3_BUCKET:-}" ]; then
        note "no S3_ENDPOINT/S3_BUCKET in $ROOT/backup.env — local copy only"
        return 0
    fi
    # Any S3-compatible endpoint will do. The object is plain text and carries no secret, so the
    # store needs to be durable, not trusted. --aws-sigv4 is in curl since 7.75; no extra client
    # to keep installed.
    if curl -sSf --retry 3 --retry-delay 5 \
            --aws-sigv4 "aws:amz:${S3_REGION:-us-east-1}:s3" \
            --user "${S3_KEY_ID}:${S3_SECRET}" \
            -H "Content-Type: application/json" \
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
    safe_path "$tmp"; safe_path "$DEST"

    sqlite3 "$DB" "VACUUM INTO '$tmp/snap.db'" || die "VACUUM INTO failed on $DB"
    [ "$(sqlite3 "$tmp/snap.db" 'PRAGMA integrity_check;')" = "ok" ] || die "snapshot fails SQLite integrity check"

    local floor count
    floor="$(cat "$STATE" 2>/dev/null || echo 0)"
    # On a copy: verify_log opens through Db::open, which applies migrations. The exported rows
    # stay the ones VACUUM produced.
    cp "$tmp/snap.db" "$tmp/check.db"
    count="$("$VERIFY" --db "$tmp/check.db" --at-least "$floor")"

    local stamp name
    stamp="$(date -u +%Y%m%dT%H%M%SZ)"
    name="vriltrainer-${stamp}-n${count}.json"
    export_json "$tmp/snap.db" "$tmp/$name" "$count"
    chmod 644 "$tmp/$name"

    # The loop closed: import what was just written rather than trusting the export.
    local back
    back="$(check_archive "$tmp/$name" "$tmp/back.db" "$count")"
    [ "$back" = "$count" ] || die "round trip lost entries: wrote $count, read back $back"
    # And the same again for everything the chain says nothing about.
    [ "$(sqlite3 "$tmp/snap.db" '.sha3sum')" = "$(sqlite3 "$tmp/back.db" '.sha3sum')" ] \
        || die "round trip changed table content: the rebuilt database does not match the snapshot"

    mv "$tmp/$name" "$DEST/$name"
    echo "$count" > "$STATE"
    printf '%s\t%s\t%s\t%s\n' "$stamp" "$count" "$(sha256sum "$DEST/$name" | cut -d' ' -f1)" "$name" >> "$INDEX"
    note "$name: $count entries, $(du -h "$DEST/$name" | cut -f1), verified through a rebuild"

    prune
    upload "$DEST/$name" "$name" || true
}

case "${1:-}" in
    "")         cmd_backup ;;
    --check)    cmd_check ;;
    --restore)  [ $# -eq 3 ] || die "usage: backup.sh --restore <archive.json> <dest.db>"; cmd_restore "$2" "$3" ;;
    *)          die "unknown argument: $1" ;;
esac
