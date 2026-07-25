# Client

The Angular application. It talks to the Rust service over the endpoints in
[`contracts/http-api.md`](../specs/001-remote-viewing-trainer/contracts/http-api.md) and computes
nothing the server is supposed to prove. The one exception is `s_client`, which is the browser's
contribution to the seed by design (D3) — and the answer echoes it back so the verification panel
can check even that.

Generated with Angular CLI 19.2.

## Running it against the server

`ng serve` proxies `/api`, `/admin` and `/pool` to `http://127.0.0.1:8080` — see
`proxy.conf.json`. Start the service first:

```bash
cargo run -p server -- --locale de --db ./dev.sqlite --pool shared/pool/v1.json
cd client && npm start          # http://localhost:4200
```

Without a server running, every page loads and every one of them says the figures are not
available. That is the intended behaviour and worth seeing at least once: there is no offline
mode and no sample data, because a page that can invent a trial cannot be told apart from one
that is checking a server.

## Two conventions this client depends on

**Pool images are served from `/pool/<image_id>.png`.** The manifest carries identifiers and
categories and deliberately nothing else — no filenames, no captions, no sizes — because anything
per-image that could differ between the target and its seven decoys is a sensory channel (D5).
The bytes themselves are what `poolctl` writes to `pool/normalised/<id>.png`, and the deployment
copies that directory into the public root beside the built bundle:

```
pool/normalised/            →  /srv/vriltrainer/public/pool/
```

Identity follows content, so those files are immutable under their own names and cache forever.
A missing one shows as a broken image and breaks nothing else.

**Category names are product copy and live here**, in `src/app/core/pool.ts`, keyed by the
manifest's category identifiers. The `.com` build carries the English list. An identifier the
list does not know renders as itself rather than as a placeholder: a pool that gained a category
before this file caught up must still label its images truthfully.

## Verification

`src/app/verify/` is an implementation of the derivation, the framing and the manifest hash
written from the specification and **not** shared with the server. A client that checks the
server with the server's own code checks nothing.

```bash
npm run conformance     # runs shared/vectors/derivation.json through the TypeScript half
```

It must report `ALL 7 CASES AGREE`. `shared/vectors/**` is frozen; if this disagrees, the client
is wrong.

## Building

```bash
npm run build           # dist/client/browser
```

## Unit tests

```bash
ng test
```

Note: the Karma target does not currently resolve the `@fontsource` `url()` in `src/styles.scss`,
which the application builder handles. That is a pre-existing gap in `angular.json`'s `test`
options rather than a failing test.
