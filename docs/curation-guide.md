# Bildpool kuratieren

Auf Deutsch, weil das die Arbeitssprache beim Kuratieren ist. Alles andere in diesem Repository
ist Englisch.

Der Pool ist der einzige Teil des Projekts, den keine Software erzeugen kann. Er entscheidet
darüber, ob die Sitzungen etwas taugen — und er blockiert alles andere, bis er steht. Diese
Anleitung ist so geschrieben, dass jemand anderes als der Betreiber ihr folgen kann.

## Was du tust, in einem Satz

Bilder finden, Herkunft und Lizenz notieren, eine Kategorie vergeben, durch das Werkzeug laufen
lassen. Fertig.

```bash
poolctl add strand.jpg --source https://commons.wikimedia.org/... --licence CC0 --category landschaft
poolctl check                 # meckert, bevor es zu spät ist
poolctl build --version 1     # normalisiert, hasht, schreibt shared/pool/v1.json
```

## Woher die Bilder kommen dürfen

**Nur CC0 und Public Domain.** Kein CC-BY, keine „sieht frei aus"-Bilder, keine Bildersuche.

| Quelle | Worauf achten |
|---|---|
| Wikimedia Commons | Lizenz steht auf der Dateiseite. „PD-old" und „CC0" sind gut, „CC-BY-SA" nicht |
| Unsplash | Eigene Lizenz, für diesen Zweck brauchbar; Quelle trotzdem notieren |
| Pexels | wie Unsplash |
| openverse | Filter auf CC0 stellen, nicht auf „alle" |

**Warum kein CC-BY:** Die Namensnennung müsste sichtbar am Bild stehen. Ein Bild von acht, das
eine Zeile Text trägt, ist dadurch von den anderen sieben unterscheidbar — und damit als Ziel
erkennbar, ohne dass irgendetwas Übersinnliches im Spiel wäre.

**Warum überhaupt so streng:** Die Seite läuft auf einer `.de`-Domain und sieht kommerziell aus.
Das macht dich für Abmahnungen bequem erreichbar. Fünf Minuten Lizenzprüfung sind billiger als
ein Brief.

## Was ein brauchbares Ziel ausmacht

Ein Bild taugt, wenn jemand mit geschlossenen Augen einen Eindruck haben und ihn danach
wiedererkennen kann.

**Nimm:** klare Motive, kräftige Formen, eindeutiger Bildgegenstand. Ein Leuchtturm. Ein
Pferd. Eine Brücke bei Nacht. Ein Teller Kirschen.

**Lass liegen:** diffuse Texturen, Nebel, Unschärfe, „ästhetische" Aufnahmen ohne Gegenstand. Wer
den Eindruck „irgendwas Graues" hat, kann zwischen drei grauen Bildern nicht wählen.

## Was nie in den Pool kommt

- **Lesbarer Text im Bild.** Schilder, Logos, Beschriftungen. Doppelt schlecht: Text markiert ein
  Bild unter acht, und auf einer zweisprachigen Seite steht ein deutsches Wort auch noch falsch
  auf der englischen Domain.
- **Erkennbare Gesichter.** Persönlichkeitsrechte, und du willst die Diskussion nicht führen.
- **Alles mit strittiger Lizenz.** Wenn du argumentieren musst, ist die Antwort nein.
- **Bilder, die du schon hast.** `poolctl check` erkennt das am Hash, aber es spart Zeit, vorher
  hinzusehen.

## Kategorien

Jedes Bild bekommt genau eine. Es gibt 16 bis 24 davon, und jede Sitzung zieht **acht
verschiedene** — deshalb sieht ein Nutzer nie zwei Bilder derselben Art nebeneinander.

**Konsistenz ist wichtiger als Genauigkeit.** Eine Kategorie ist ein Ziehungstopf, keine
Taxonomie. Wenn ein Bild in zwei passt, nimm die, in der du es später suchen würdest, und bleib
dabei. Ein Wasserfall ist entweder immer „landschaft" oder immer „wasser" — nur nicht mal so und
mal so.

Faustregel für den Zuschnitt: Zwei Bilder derselben Kategorie sollten sich noch deutlich
unterscheiden. Wenn „landschaft" nur noch Berge enthält, ist es Zeit, „gebirge" abzuspalten.

## Vielfalt innerhalb der Kategorie

Das ist der Punkt, den die Kategorien **nicht** lösen und den nur du lösen kannst.

Die Ziehung sorgt dafür, dass ein Set aus acht verschiedenen Kategorien besteht. Sie kann nichts
dagegen tun, dass deine zwanzig Landschaftsbilder alle Küsten bei Sonnenuntergang sind. Dann
kommen zwar nie zwei gleichzeitig vor — aber über hundert Sitzungen wird es eintönig, und die
Leute hören auf.

Wenn du eine Kategorie auffüllst, schau dir die vorhandenen zwanzig an, bevor du das
einundzwanzigste hinzufügst.

## Herkunft sofort notieren

Beim Hinzufügen, nicht später. Eine verlorene Quell-URL ist nicht wiederherstellbar, und sie ist
das Einzige, womit du die Lizenz belegen kannst. `poolctl check` verweigert Bilder ohne Quelle
und ohne Lizenz — nicht aus Prinzipienreiterei, sondern weil das die einzige Stelle ist, an der
es noch billig ist.

## Wann eine neue Version geschnitten wird

Pool-Versionen sind unveränderlich. Jede Sitzung merkt sich, unter welcher Version sie lief, und
bleibt dagegen für immer überprüfbar. Deshalb:

- Sammle in Ruhe, schneide dann in einem Rutsch.
- Sinnvoll ist ein Schnitt bei etwa fünfzig neuen Bildern oder wenn eine Kategorie neu dazukommt.
- Nach dem Schneiden nichts mehr an der Version anfassen. Auch keine Kleinigkeit — die Reihenfolge
  im Manifest bestimmt, welches Bild welche Nummer hat, und die Nummern bestimmen jede künftige
  Ziehung. Ein „nur schnell umsortiert" verändert rückwirkend, was in Sitzungen hätte gezogen
  werden sollen.

## Der Startpool

Fünfhundert Bilder, verteilt auf 16 bis 24 Kategorien, also grob zwanzig pro Kategorie.

Das sind mehrere Abende Arbeit. Es gibt keine Abkürzung: die Pipeline lässt sich automatisieren,
die Auswahl nicht. Fang früh an — bis der Pool steht, ist nichts anderes spielbar.
