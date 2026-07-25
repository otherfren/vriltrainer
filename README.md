# vriltrainer

A public remote viewing test. A coordinate, eight images, one choice — and a proof that the target
was fixed beforehand. The whole record is downloadable, and recomputing it is the point.
Bilingual: `vriltrainer.de` in German, `vriltrainer.com` in English.

The expected result is 12.5%, which is nothing. The site is built to say so.

**Status: under construction.** The server has no HTTP layer yet and there is no image pool. What
is documented below works; what does not exist is listed under [What is
missing](#what-is-missing).

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

# 3 — the interface in development mode, http://localhost:4200
cd client && npm start
```

The interface currently runs in **demo mode** and says so: it produces coordinates and outcomes
locally, because there is no server for it to talk to yet. The design and the flow are real, the
numbers are not.

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

A static binary plus a database file, behind the nginx already running on the host. No container,
no runtime.

```bash
# build
cargo build --release
cd client && npm run build && cd ..

# upload
scp target/release/server         srv:/srv/vriltrainer/server
scp -r client/dist/client/browser srv:/srv/vriltrainer/public
scp shared/pool/v1.json           srv:/srv/vriltrainer/pool/v1.json

# one-time setup
scp deploy/vriltrainer.service    srv:/etc/systemd/system/
scp deploy/nginx.conf             srv:/etc/nginx/sites-available/vriltrainer
ssh srv 'ln -sf /etc/nginx/sites-available/vriltrainer /etc/nginx/sites-enabled/ \
         && systemctl daemon-reload && systemctl enable --now vriltrainer \
         && nginx -t && systemctl reload nginx'
```

**Two headers decide whether this works**, and both break silently when absent. Without a
forwarded client address the service sees `127.0.0.1` for everyone, so the limit on account
creation is either inert or throttles all users together. Without an unchanged `Host` it cannot
select the language build. Both are set in `deploy/nginx.conf`.

**One secret** belongs in `/etc/vriltrainer/env`: the key for the trial tokens. Everything else in
the database is public by design.

```
VRILTRAINER_TOKEN_KEY=<32 random bytes, hex>
```

**The backup is not an operational detail.** The SQLite file *is* the public audit log. Losing it
does not cost user data that can be rebuilt — it retroactively removes the verifiability of every
past trial. The existing dump-to-S3 script covers this; the bucket must not be publicly readable,
because a dump contains `s_server` for trials still in flight.

## What is missing

| | |
|---|---|
| HTTP layer | Derivation, commitment, tokens and the hash chain exist and are tested, but no route reaches them |
| Image pool | 500+ curated images. The largest piece of manual work, and it blocks every playable trial. Guide: [`docs/curation-guide.md`](docs/curation-guide.md) |
| `poolctl` | Normalising, annotating, writing the manifest |
| Statistics, leaderboard, log export | Server side |
| Second language | English bundles and the language switch |

Full list: [`specs/001-remote-viewing-trainer/tasks.md`](specs/001-remote-viewing-trainer/tasks.md).

## Where things live

| | |
|---|---|
| `docs/trial-protocol-decisions.md` | D1–D22 — why the protocol looks the way it does |
| `docs/curation-guide.md` | Curating the image pool |
| `specs/001-remote-viewing-trainer/` | Specification, plan, data model, contracts |
| `shared/vectors/` | Derivation specification and test vectors — the contract between the two implementations |
| `server/` | Rust service |
| `client/` | Angular interface |
| `tools/poolctl/` | Curation tool |

## Licence

AGPL-3.0-or-later. Anyone running a modified version must publish their changes — for a service
whose entire promise is verifiability, that is not a formality.
