# vriltrainer

A public remote viewing test. A coordinate, eight images, one choice — and a proof that the target
was fixed beforehand. The whole record is downloadable, and recomputing it is the point.
Bilingual: `vriltrainer.de` in German, `vriltrainer.com` in English.

The expected result is 12.5%, which is nothing. The site is built to say so.

**Status: under construction.** The server serves the whole API and the client talks to it; the
image pool is still being curated. What is documented below works; what does not exist is listed
under [What is missing](#what-is-missing).

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
cargo run --bin poolctl -- import
cargo run --bin poolctl -- check                       # what is thin, what would block a version
cargo run --bin poolctl -- build --version 1 --out pool/v1.json

# 2 — the one secret. Everything else in the database is public by design
openssl rand -hex 32 > token.key && chmod 600 token.key

# 3 — run it
cargo run --bin server -- \
  --locale de --db dev.db --pool pool/v1.json \
  --listen 127.0.0.1:8080 --token-key token.key \
  --public client/dist/client/browser
```

`--public` serves the built bundle, so this is the whole product on one port; leave it off to run
the API alone. `/api` and `/admin` are routed before the bundle, so an unknown `/api/...` is still
a `404` and not the web page.

The images themselves are **not** served by the manifest — it carries ids and no filenames (D5).
Copy `pool/normalised/` to `<public>/pool/` and the client finds them at `/pool/<image_id>.png`:

```bash
mkdir -p client/dist/client/browser/pool && cp pool/normalised/*.png client/dist/client/browser/pool/
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
target/release/server        →  /srv/vriltrainer/server
client/dist/client/browser   →  /srv/vriltrainer/public
pool/normalised/             →  /srv/vriltrainer/public/pool/
pool/v1.json                 →  /srv/vriltrainer/pool/manifest.json
pool/v*.json                 →  /srv/vriltrainer/pool/            (every version, unrenamed)
deploy/vriltrainer@.service  →  /etc/systemd/system/
deploy/nginx.conf            →  /etc/nginx/sites-available/vriltrainer
```

The manifest is copied **twice**, and both copies are load-bearing. `manifest.json` is the version
the process serves, named by `--pool` in the unit. The `v<N>.json` files beside it are what
`GET /api/pool/{version}/manifest` reads older versions from — a trial recorded under v1 stays
verifiable only while v1's manifest still answers (D5), so dropping the old file after a pool bump
breaks the audit story for every trial before it, silently and retroactively.

`pool/normalised/` is the image bytes. The manifest carries ids and no filenames, so nothing but
this convention connects the two: the client asks for `/pool/<image_id>.png`.

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
| Image pool | 160+ curated images at launch (D5, amended down from 500). The largest piece of manual work, and it blocks every playable trial. Guide: [`docs/curation-guide.md`](docs/curation-guide.md) |
| `DELETE /api/account/name` | The only contracted route with no module. It answers `501`, not `404`, and says so |
| Rank recomputation timer | The fifteen-minute pass exists but is triggered from the read side rather than by a scheduler |
| English message catalogue | The switch and the handoff work; the `en` strings are not written |

Full list: [`specs/001-remote-viewing-trainer/tasks.md`](specs/001-remote-viewing-trainer/tasks.md).

## Where things live

| | |
|---|---|
| `docs/trial-protocol-decisions.md` | D1–D28 — why the protocol looks the way it does |
| `docs/curation-guide.md` | Curating the image pool |
| `specs/001-remote-viewing-trainer/` | Specification, plan, data model, contracts |
| `shared/vectors/` | Derivation specification and test vectors — the contract between the two implementations |
| `server/` | Rust service |
| `client/` | Angular interface |
| `tools/poolctl/` | Curation tool |

## Licence

AGPL-3.0-or-later. Anyone running a modified version must publish their changes — for a service
whose entire promise is verifiability, that is not a formality.
