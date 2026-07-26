# vriltrainer

A public remote viewing test. A coordinate, eight images, one choice — and a proof that the target
was fixed beforehand. The whole record is downloadable, and recomputing it is the point.
Bilingual: `vriltrainer.de` in German, `vriltrainer.com` in English.

The expected result is 12.5%, which is nothing. The site is built to say so.

**Status: live and unfinished.** Both domains are served, the API is complete but for one route,
and the image pool is published at **v2, 200 images in 19 categories**. What is documented below
works; what does not exist is listed under [What is missing](#what-is-missing).

## Requirements

- **Rust 1.95+** (`cargo`)
- **Node 22+** — on this machine under `~/.local/node`, linked into `~/.local/bin` so it is on
  the PATH in login and interactive shells alike. Check with `node --version`; if that fails,
  open a new terminal.

## Running locally

```bash
# 1 — all Rust tests, including derivation conformance
cargo test

# 2 — check that Rust and TypeScript compute the same derivation.
#     This is the most important check in the project. If it diverges, verification
#     fails in production on honest trials.
cd client && npm run conformance && cd ..

# 3 — the client's unit tests. Karma needs to be told which browser to use; there is no
#     bundled Chrome, so point it at the system one.
cd client && CHROME_BIN=$(command -v chromium) npm test -- --watch=false --browsers=ChromeHeadless

# 4 — the interface in development mode, http://localhost:4200
cd client && npm start
```

### Running the server

The server will not start without a **pool manifest**. That is deliberate: a process with no pool
cannot serve a trial, and failing at startup is better than failing at the first user. The manifest
is not in the repository — it is built from the catalogue, which is curated image files the
operator supplies.

```bash
# 1 — take the images named by pool/images.toml into the catalogue, then cut a version
cargo run --bin poolctl -- import                      # → pool/normalised/, the build's image input
cargo run --bin poolctl -- check                       # what is thin, what would block a version
cargo run --bin poolctl -- build --version 2 --out pool/v2.json   # v2 is the published one
cp pool/v2.json pool/manifest.json                     # what --pool names

# 2 — the one secret. Everything else in the database is public by design
openssl rand -hex 32 > token.key && chmod 600 token.key

# 3 — run it
cargo run --bin server -- \
  --locale de --db dev.db --pool pool/manifest.json \
  --listen 127.0.0.1:8080 --token-key token.key \
  --public client/dist/client/browser
```

`--public` serves the built bundle, so this is the whole product on one port; leave it off to run
the API alone. `/api` and `/admin` are routed before the bundle, so an unknown `/api/...` is still
a `404` and not the web page.

**Run `poolctl import` before `cargo build`.** The images are compiled into the binary (D29): the
build reads `pool/normalised/` and the server serves them from memory at `/pool/<image_id>.png`.
There is nothing to copy and nothing to mount, and a manifest naming an image the binary does not
carry is refused at startup rather than discovered by a visitor looking at eight broken pictures.

A checkout with no images builds anyway — `pool/normalised/` is generated and not in the repository,
the tests do not need it, and `cargo` prints a warning saying the pool is empty. Set
`VRILTRAINER_POOL_IMAGES` to build from somewhere else. The startup line reports both numbers, and
they have to agree:

```
pool_version=2 pool_images=200 pool_bytes_embedded=200
```

A quick check that it is alive, which is also the shape of the loop:

```bash
curl -s localhost:8080/api/health
TOK=$(curl -s -XPOST localhost:8080/api/account -H 'content-type: application/json' \
        -d '{"name":"tester"}' | jq -r .access_token)
curl -s -XPOST localhost:8080/api/trial -H "Authorization: Bearer $TOK"
```

`POST /api/trial/reveal` then takes that token plus 32 base64 bytes of `s_client`, and
`/api/trial/answer` takes the token it returns plus the chosen image — **no sooner than three
seconds after the reveal**, or it answers `425` and the trial stays open. The full shapes are in
[`contracts/http-api.md`](specs/001-remote-viewing-trainer/contracts/http-api.md).

### Building the production bundle

```bash
cd client && npm run build     # → client/dist/client/browser
```

### Regenerating the test vectors

Deliberately only, never as part of a build:

```bash
cargo run --bin gen_vectors > shared/vectors/derivation.json
```

This changes the contract both implementations are held to and makes any already published trial
unverifiable. See `shared/vectors/README.md`.

## Deploying

Two processes, one machine, one database. `vriltrainer.de` and `vriltrainer.com` are the same
binary started twice with a different `--locale` (D24). No container, no runtime.

### Build

```bash
cargo build --release
cd client && npm run build && cd ..
```

### Copy

```
target/release/server           →  /srv/vriltrainer/server
client/dist/client/browser/de   →  /srv/vriltrainer/public_de
client/dist/client/browser/en   →  /srv/vriltrainer/public_en
pool/v2.json                    →  /srv/vriltrainer/pool/manifest.json
pool/v<N>.json                  →  /srv/vriltrainer/pool/         (every version, unrenamed)
deploy/vriltrainer@.service     →  /etc/systemd/system/
deploy/nginx.conf               →  /etc/nginx/sites-available/vriltrainer
```

**There are two bundles, not one.** Since the client is translated, `npm run build` writes one per
language and `--public` names the **language**, not the instance. They carry the same filenames with
different contents — Angular hashes before translating — so a cache keyed on path alone, or a single
shared `public/`, serves one domain in the wrong language, silently, while everything else works.

The manifest is copied **twice**, and both copies are load-bearing. `manifest.json` is the version
the process serves, named by `--pool` in the unit. The `v<N>.json` files beside it are what
`GET /api/pool/{version}/manifest` reads older versions from — a trial recorded under v1 stays
verifiable only while v1's manifest still answers (D5), so dropping the old file after a pool bump
breaks the audit story for every trial before it, silently and retroactively.

The image bytes are **not** copied: they are inside `server`, put there at compile time from
`pool/normalised/` (D29). So a pool bump is a rebuild, not a file sync — which is the honest shape
of it, since a published pool version is immutable and a different pool is a different artefact.
The manifest still carries ids and no filenames; the client asks for `/pool/<image_id>.png` and the
binary answers.

### Configure

```
/etc/vriltrainer/token.key   64 hex characters, mode 0600, owned by root
/etc/vriltrainer/de.env      LISTEN=127.0.0.1:8080
/etc/vriltrainer/en.env      LISTEN=127.0.0.1:8081
```

The token key is the one secret in the deployment; everything else in the database is public by
design. Lose it and every trial in flight becomes uncompletable.

### Start

```bash
systemctl daemon-reload
systemctl enable --now vriltrainer@de vriltrainer@en
ln -sf /etc/nginx/sites-available/vriltrainer /etc/nginx/sites-enabled/
nginx -t && systemctl reload nginx
```

Each instance walks the hash chain at startup and refuses to run if it does not link. That is the
intended behaviour: appending to a record that is already wrong is worse than being down.

### Three things that break silently

**The forwarded client address.** Without it the service sees `127.0.0.1` for everyone, so the
limit on account creation is either inert or throttles all users together. `--trusted-proxy` in
the unit must be the address nginx connects from. Since D24 the `Host` header is no longer
load-bearing — the locale is fixed by the flag the process was started with, and a German
instance cannot serve English at all.

**One machine.** Both processes write to the same SQLite file, and the two-writer append
discipline the hash chain depends on (R9) assumes local-filesystem locking. On NFS it will appear
to work and will fork the log.

**The backup.** The SQLite file *is* the public audit log. Losing it does not cost user data that
can be rebuilt — it retroactively removes the verifiability of every past trial. `deploy/backup.sh`
is the job; see below for what it refuses and why.

### Backups

```
deploy/backup.sh                    →  /srv/vriltrainer/backup.sh
deploy/backup.env.example           →  /srv/vriltrainer/backup.env     (fill in, mode 0600)
deploy/vriltrainer-backup.service   →  /etc/systemd/system/
deploy/vriltrainer-backup.timer     →  /etc/systemd/system/
target/release/verify_log           →  /srv/vriltrainer/verify_log
```

```bash
systemctl enable --now vriltrainer-backup.timer
```

Hourly. Each run takes a `VACUUM INTO` snapshot, walks its hash chain, gzips it, then **unpacks
what it just wrote and walks the chain again** before keeping it. Retention is every snapshot for a
week, one a day for ninety days, one a month after that.

Four refusals, each for something a `cp` does not survive:

- **`VACUUM INTO`, not a file copy.** In WAL mode almost nothing is in the main file — a live `.db`
  of 4 KB beside a 3 MB `-wal` is normal. Copying the `.db` alone backs up an empty database;
  copying all three under a concurrent writer backs up three files that need not agree.
- **The chain is walked, not assumed.** `verify_log --db <path>` is that walk on its own: exit 0
  verifies, 1 does not, 2 unreadable. Point it at a *copy* — opening a database applies pending
  migrations, so pointing it at an archive rewrites the artefact under test.
- **A snapshot shorter than the last one is refused.** The first N entries of a valid log are
  themselves a valid log, so a chain walk cannot see a missing tail. Only the count comparison can,
  and it is kept in `backups/.last-count`.
- **The round trip is exercised every hour, not at restore time.** An archive that cannot be
  unpacked and re-verified is caught the hour it is written, not the day it is needed.

`backup.sh --check` verifies the newest archive and changes nothing. `backup.sh --restore <archive>
<dest.db>` unpacks, verifies, and refuses to overwrite.

**The archives are plain gzip and are not encrypted.** Nothing in the schema is a secret: access
tokens, handoff codes and admin keys are stored only as hashes, and the log carries the opaque
account id rather than a name. What matters about this database is that it survives and still
verifies, not that it stays unread — it is the record every past trial is checked against, and it
is meant to be checkable by anyone.

Off-site is optional and configured in `backup.env` — any S3-compatible endpoint, uploaded through
`curl --aws-sigv4`. A failed upload warns and does not stop the next snapshot; it must never be the
reason no backup was taken. A trial in flight is not a hazard here: `s_server` is written only on
the resolve row, which publishes it deliberately, and the table constraint forbids it on a commit
row. The pending half lives in the sealed token the client holds, not in the database.

## What is missing

| | |
|---|---|
| `DELETE /api/account/name` | The only contracted route with no module. It answers `501`, not `404`, and says so |
| Rank recomputation timer | The fifteen-minute pass exists but is triggered from the read side rather than by a scheduler (T102, D23) |
| Thin categories | The pool clears the launch bar (200, over the 160 of D5), but 13 of the 19 categories hold fewer than 12 images and will repeat visibly. `poolctl check` names them. Guide: [`docs/curation-guide.md`](docs/curation-guide.md) |
| TLS in the shipped nginx config | `deploy/nginx.conf` has the `ssl_certificate` lines commented out; certificates are per deployment |

Both message catalogues are written and complete — `node client/tools/build-en-catalogue.mjs --check`
is what says so, and it fails on a gap rather than shipping an untranslated string.

Full list: [`specs/001-remote-viewing-trainer/tasks.md`](specs/001-remote-viewing-trainer/tasks.md)
— 84 of 115 ticked. Every open line was audited against the code on 2026-07-26; where the code is
further along than the line, the line now says how far and what is actually missing.

## Where things live

| | |
|---|---|
| `docs/trial-protocol-decisions.md` | D1–D34 — why the protocol looks the way it does |
| `docs/curation-guide.md` | Curating the image pool |
| `docs/launch-plan.md` | What has to be true before this is put in front of people |
| `specs/001-remote-viewing-trainer/` | Specification, plan, data model, contracts |
| `shared/vectors/` | Derivation specification and test vectors — the contract between the two implementations |
| `server/` | Rust service |
| `client/` | Angular interface |
| `tools/poolctl/` | Curation tool |

## Licence

AGPL-3.0-or-later. Anyone running a modified version must publish their changes — for a service
whose entire promise is verifiability, that is not a formality.
