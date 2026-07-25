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

# 3 — the client's unit tests. Karma needs to be told which browser to use; there is no
#     bundled Chrome, so point it at the system one.
cd client && CHROME_BIN=$(command -v chromium) npm test -- --watch=false --browsers=ChromeHeadless

# 4 — the interface in development mode, http://localhost:4200
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

Two processes, one machine, one database. `vriltrainer.de` and `vriltrainer.com` are the same
binary started twice with a different `--locale` (D24). No container, no runtime.

### Build

```bash
cargo build --release
cd client && npm run build && cd ..
```

### Copy

```
target/release/server        →  /srv/vriltrainer/server
client/dist/client/browser   →  /srv/vriltrainer/public
shared/pool/v1.json          →  /srv/vriltrainer/pool/manifest.json
deploy/vriltrainer@.service  →  /etc/systemd/system/
deploy/nginx.conf            →  /etc/nginx/sites-available/vriltrainer
```

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
can be rebuilt — it retroactively removes the verifiability of every past trial. Restore into a
scratch database and walk the chain occasionally; an untested backup of an audit log is an
untested product promise. The bucket must not be publicly readable, because a dump contains
`s_server` for trials still in flight.

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
