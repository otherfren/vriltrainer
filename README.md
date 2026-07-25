# vriltrainer

Ein öffentlicher Remote-Viewing-Test. Koordinate, acht Bilder, eine Wahl — und ein Nachweis, dass
das Ziel schon vorher feststand. Der gesamte Datensatz ist herunterladbar, nachrechnen ist der
Zweck. Zweisprachig auf `vriltrainer.de` (Deutsch) und `vriltrainer.com` (Englisch).

Das erwartete Ergebnis ist 12,5 %, also nichts. Die Seite ist darauf ausgelegt, das zu sagen.

**Stand: im Bau.** Der Server hat noch kein HTTP, es gibt noch keinen Bildpool. Was unten steht,
funktioniert; was fehlt, steht unter [Was noch fehlt](#was-noch-fehlt).

## Voraussetzungen

- **Rust 1.95+** (`cargo`)
- **Node 22+** — auf dieser Maschine unter `~/.local/node`, PATH-Zeile steht in `~/.bashrc`

## Lokal

```bash
# 1 — alle Rust-Tests, inklusive der Ableitungs-Konformanz
cargo test

# 2 — prüfen, dass Rust und TypeScript dieselbe Ableitung rechnen
#     Das ist die wichtigste Prüfung im Projekt. Weicht sie ab, scheitert
#     in Produktion die Verifikation ehrlicher Sitzungen.
npx tsx client/conformance.mts

# 3 — Oberfläche im Entwicklungsmodus, http://localhost:4200
cd client && npm start
```

Die Oberfläche läuft derzeit im **Demo-Modus** und sagt das auch: sie erzeugt Koordinaten und
Ergebnisse lokal, weil es noch keinen Server gibt, mit dem sie sprechen könnte. Das Design und der
Ablauf sind echt, die Zahlen nicht.

### Produktionsbündel bauen

```bash
cd client && npm run build     # → client/dist/client/browser
```

### Testvektoren neu erzeugen

Nur bewusst, nie als Teil eines Builds:

```bash
cargo run --bin gen_vectors > shared/vectors/derivation.json
```

Das ändert den Vertrag, an den beide Implementierungen gebunden sind, und macht bereits
veröffentlichte Sitzungen unüberprüfbar. Siehe `shared/vectors/README.md`.

## Deployen

Ein statisches Binary plus eine Datenbankdatei, hinter dem nginx, das auf dem Hetzner-Server
ohnehin läuft. Kein Container, keine Runtime.

```bash
# bauen
cargo build --release
cd client && npm run build && cd ..

# hochladen
scp target/release/server        srv:/srv/vriltrainer/server
scp -r client/dist/client/browser srv:/srv/vriltrainer/public
scp shared/pool/v1.json          srv:/srv/vriltrainer/pool/v1.json

# einrichten, einmalig
scp deploy/vriltrainer.service   srv:/etc/systemd/system/
scp deploy/nginx.conf            srv:/etc/nginx/sites-available/vriltrainer
ssh srv 'ln -sf /etc/nginx/sites-available/vriltrainer /etc/nginx/sites-enabled/ \
         && systemctl daemon-reload && systemctl enable --now vriltrainer \
         && nginx -t && systemctl reload nginx'
```

**Zwei Header entscheiden, ob es funktioniert**, und beide brechen lautlos, wenn sie fehlen. Ohne
weitergereichte Client-Adresse sieht der Dienst für jeden `127.0.0.1`, und das Limit auf
Account-Erstellung ist entweder wirkungslos oder drosselt alle gemeinsam. Ohne unverändertes
`Host` kann er die Sprachversion nicht wählen. Beides steht fertig in `deploy/nginx.conf`.

**Ein Geheimnis** gehört nach `/etc/vriltrainer/env`: der Schlüssel für die Trial-Token. Alles
andere in der Datenbank ist öffentlich vorgesehen.

```
VRILTRAINER_TOKEN_KEY=<32 zufällige Bytes, hex>
```

**Das Backup ist kein Betriebsdetail.** Die SQLite-Datei *ist* das öffentliche Audit-Log. Geht sie
verloren, sind nicht Nutzerdaten weg, sondern rückwirkend die Überprüfbarkeit jeder bisherigen
Sitzung. Das vorhandene Dump-nach-S3-Skript deckt das ab — der Bucket darf nicht öffentlich
lesbar sein, weil ein Dump `s_server` laufender Sitzungen enthält.

## Was noch fehlt

| | |
|---|---|
| HTTP-Schicht | Ableitung, Commitment, Token und Hash-Kette stehen und sind getestet, sind aber über keine Route erreichbar |
| Bildpool | 500+ kuratierte Bilder. Der größte Handarbeitsbrocken, blockiert jede spielbare Sitzung. Anleitung: [`docs/curation-guide.md`](docs/curation-guide.md) |
| `poolctl` | Normalisieren, annotieren, Manifest schreiben |
| Statistik, Bestenliste, Log-Export | serverseitig |
| Zweite Sprache | Englische Bündel und der Sprachwechsel |

Vollständige Liste: [`specs/001-remote-viewing-trainer/tasks.md`](specs/001-remote-viewing-trainer/tasks.md).

## Wo was steht

| | |
|---|---|
| `docs/trial-protocol-decisions.md` | D1–D22 — warum das Protokoll so aussieht, wie es aussieht |
| `docs/curation-guide.md` | Bildpool kuratieren |
| `specs/001-remote-viewing-trainer/` | Spezifikation, Plan, Datenmodell, Contracts |
| `shared/vectors/` | Ableitungs-Spezifikation und Testvektoren — der Vertrag zwischen beiden Implementierungen |
| `server/` | Rust-Dienst |
| `client/` | Angular-Oberfläche |
| `tools/poolctl/` | Kurationswerkzeug |

## Lizenz

AGPL-3.0-or-later. Wer eine veränderte Fassung betreibt, muss die Änderungen offenlegen — bei
einem Dienst, dessen ganzes Versprechen Überprüfbarkeit ist, ist das keine Formalität.
