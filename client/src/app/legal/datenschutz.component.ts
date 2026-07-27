import { Component } from '@angular/core';
import { RouterLink } from '@angular/router';

/**
 * Article 13 GDPR, and Article 16 DSA at the bottom.
 *
 * Written to be read rather than to be complete, because a page nobody finishes protects nobody.
 * Every claim here is one the code actually keeps, and the two that are unusual are stated plainly
 * instead of being buried: the public record carries the account identifier and never the name
 * (FR-026), and a session cannot be deleted because the log is an append-only hash chain — which
 * is why the *name* can be, and why that is the erasure this site offers (FR-035, D33 · D9).
 *
 * The retention figures are the ones the machine really applies: thirty days for an account that
 * never played (D32), seven days for the web server's access log, which is the only place an IP
 * address appears at all — the service's own log records a request id, a method and a path, and
 * `http::trace::safe_target` cuts the query and the fragment before it does.
 */
@Component({
  selector: 'app-datenschutz',
  standalone: true,
  imports: [RouterLink],
  template: `
    <section class="shell legal">
      <p class="eyebrow" i18n="@@privacy.eyebrow">Artikel 13 DSGVO</p>
      <h1 i18n="@@privacy.heading">Datenschutz</h1>

      <div class="legal__card panel">
        <p class="legal__lead" i18n="@@privacy.lead">
          Diese Seite speichert so wenig wie möglich. Was sie speichert, steht hier vollständig.
        </p>

        <h2 i18n="@@privacy.short.h">Das Wichtigste zuerst</h2>
        <ul class="legal__list">
          <li i18n="@@privacy.short.1">
            Kein Tracking, keine Cookies, keine Werbung, keine Weitergabe zu Werbezwecken.
          </li>
          <li i18n="@@privacy.short.2">
            Keine E-Mail-Adresse, kein Passwort, kein Name aus deinem Ausweis - du wählst selbst,
            wie du heißt.
          </li>
          <li i18n="@@privacy.short.3">
            Im öffentlichen Protokoll steht <strong>nie dein Name</strong>, sondern eine zufällige
            Kennung.
          </li>
          <li i18n="@@privacy.short.4">
            Deinen Namen kannst du jederzeit löschen. Deine Sitzungen bleiben - warum, steht weiter
            unten.
          </li>
        </ul>

        <h2 i18n="@@privacy.who.h">Wer verantwortlich ist</h2>
        <p i18n="@@privacy.who.p">
          Die im <a routerLink="/impressum">Impressum</a> genannte Person. Dort stehen auch
          Anschrift und Kontakt.
        </p>

        <h2 i18n="@@privacy.what.h">Was gespeichert wird</h2>
        <p i18n="@@privacy.what.account">
          <strong>Zu deinem Konto:</strong> der Name, den du dir gibst, eine zufällige öffentliche
          Kennung, und der Prüfwert deines Zugangslinks. Den Link selbst speichern wir nicht - wir
          könnten dein Konto also nicht wiederherstellen, selbst wenn wir wollten.
        </p>
        <p i18n="@@privacy.what.trials">
          <strong>Zu jeder Sitzung:</strong> Zeitpunkt, Koordinate, die Festlegung auf das Ziel, das
          Bild, das du gewählt hast, das Ziel und ob es ein Treffer war. Das ist der Datensatz, den
          die Seite öffentlich nachprüfbar macht.
        </p>
        <p i18n="@@privacy.what.logs">
          <strong>Beim Aufruf:</strong> der Webserver protokolliert deine IP-Adresse, Zeitpunkt und
          aufgerufene Adresse. Der Dienst selbst protokolliert <em>keine</em> IP-Adresse - nur
          Anfragekennung, Methode und Pfad, und der Zugangslink wird dabei abgeschnitten, bevor
          etwas geschrieben wird.
        </p>

        <h2 i18n="@@privacy.public.h">Was öffentlich sichtbar ist</h2>
        <p i18n="@@privacy.public.p">
          Auf der Bestenliste stehen dein Name und deine öffentliche Kennung. Im herunterladbaren
          Protokoll steht nur die Kennung - dort taucht dein Name an keiner Stelle auf. Ein neuer
          Name wird erst nach einer Prüfung durch einen Menschen veröffentlicht; bis dahin steht auf
          öffentlichen Seiten eine Maske.
        </p>

        <h2 i18n="@@privacy.why.h">Rechtsgrundlage</h2>
        <p i18n="@@privacy.why.p">
          Konto und Sitzungen: Artikel 6 Absatz 1 Buchstabe b DSGVO - ohne sie gibt es den Dienst
          nicht, den du benutzen willst. Zugriffsprotokolle: Artikel 6 Absatz 1 Buchstabe f - ein
          Server, der seine Aufrufe nicht sieht, lässt sich weder betreiben noch gegen Missbrauch
          verteidigen.
        </p>

        <h2 i18n="@@privacy.keep.h">Wie lange</h2>
        <ul class="legal__list">
          <li i18n="@@privacy.keep.1">
            <strong>Konto ohne einzige Sitzung:</strong> wird nach 30 Tagen automatisch gelöscht,
            mitsamt Name und Kennung.
          </li>
          <li i18n="@@privacy.keep.2">
            <strong>Zugriffsprotokoll des Webservers:</strong> 7 Tage, dann wird es überschrieben.
          </li>
          <li i18n="@@privacy.keep.3">
            <strong>Sitzungen im öffentlichen Protokoll:</strong> dauerhaft. Der nächste Abschnitt
            sagt, warum.
          </li>
        </ul>

        <h2 i18n="@@privacy.erase.h">Löschen</h2>
        <p i18n="@@privacy.erase.name">
          <strong>Deinen Namen kannst du jederzeit löschen</strong>, im Menü hinter deinem Namen
          oben. Danach steht an seiner Stelle überall nur noch die Kennung. Das ist sofort wirksam
          und lässt sich nicht rückgängig machen.
        </p>
        <p i18n="@@privacy.erase.trials">
          <strong>Die Sitzungen selbst lassen sich nicht löschen</strong>, und das ist der Kern
          dieser Seite: Das Protokoll ist eine Kette, in der jeder Eintrag den vorherigen absichert.
          Ein Eintrag, den man herausnehmen kann, würde jede Zusage entwerten, die alle anderen
          Einträge geben - die Seite könnte dann nicht mehr belegen, dass das Ziel vor deiner Wahl
          feststand. Deshalb steht in dieser Kette von Anfang an kein Name, sondern nur eine
          zufällige Kennung. Nach der Löschung des Namens ist der Datensatz weiterhin nachprüfbar,
          aber niemandem mehr zuzuordnen.
        </p>
        <p i18n="@@privacy.erase.rest">
          Wenn du dich abmeldest, ohne den Link gespeichert zu haben, kommt niemand mehr an das
          Konto - auch wir nicht.
        </p>

        <h2 i18n="@@privacy.others.h">Wer die Daten sonst sieht</h2>
        <p i18n="@@privacy.others.p">
          Der Server steht bei der Hetzner Online GmbH in Deutschland. Davor liegt Cloudflare, das
          den Verkehr weiterleitet und dabei IP-Adressen verarbeitet; dabei können Daten in
          Drittländer übertragen werden. Weitere Empfänger gibt es nicht - keine Analysedienste,
          keine Werbenetzwerke, keine sozialen Netzwerke.
        </p>

        <h2 i18n="@@privacy.cookies.h">Cookies</h2>
        <p i18n="@@privacy.cookies.p">
          Keine. Dein Zugangslink liegt im lokalen Speicher deines Browsers, damit du nach dem
          Neuladen noch angemeldet bist - technisch notwendig, deshalb fragt dich diese Seite auch
          nicht um Erlaubnis dafür. Beim Abmelden wird er dort gelöscht.
        </p>

        <h2 i18n="@@privacy.rights.h">Deine Rechte</h2>
        <p i18n="@@privacy.rights.p">
          Du hast das Recht auf Auskunft, Berichtigung, Löschung, Einschränkung der Verarbeitung,
          Datenübertragbarkeit und Widerspruch. Für die Auskunft brauchst du nichts zu beantragen:
          Der vollständige Datensatz zu deiner Kennung steht im Protokoll, das du unten auf jeder
          Seite herunterladen kannst. Für alles andere genügt eine Nachricht an die Adresse im
          Impressum. Du kannst dich außerdem bei einer Datenschutz-Aufsichtsbehörde beschweren.
        </p>

        <h2 i18n="@@privacy.dsa.h">Rechtswidrige Inhalte melden</h2>
        <p i18n="@@privacy.dsa.p">
          Der einzige Inhalt, den Nutzer hier veröffentlichen können, ist ihr Anzeigename. Wenn dir
          einer rechtswidrig erscheint, melde ihn an die Adresse im Impressum - das ist die
          Kontaktstelle nach Artikel 16 der Verordnung über digitale Dienste. Nenne dabei den Namen
          oder die öffentliche Kennung und in einem Satz, was das Problem ist. Du bekommst eine
          Antwort, und wenn die Meldung zutrifft, wird der Name entfernt. Namen werden ohnehin vor
          der Veröffentlichung von einem Menschen geprüft.
        </p>

        <h2 i18n="@@privacy.note.h">Was diese Seite ist</h2>
        <p i18n="@@privacy.note.p">
          Ein öffentliches Experiment. Die Trefferquote liegt bei reinem Raten bei 12,5 %, und genau
          das ist das erwartete Ergebnis. Die Seite behauptet nichts anderes und weist niemandem
          übersinnliche Fähigkeiten nach - sie rechnet nach, ob die Zahlen vom Zufall abweichen, und
          veröffentlicht alles, was zum Nachrechnen nötig ist.
        </p>
      </div>
    </section>
  `,
  styles: [
    `
      .legal {
        padding: var(--s12) 0 var(--s8);
      }
      .legal__card {
        padding: var(--s6);
        max-width: 52rem;
      }
      .legal__card h2 {
        font-family: var(--shout);
        font-size: 1.375rem;
        margin: var(--s6) 0 var(--s3);
      }
      .legal__card h2:first-child {
        margin-top: 0;
      }
      .legal__card p {
        font-size: 0.9375rem;
        max-width: 62ch;
      }
      /* The lead is the one sentence somebody reads if they read nothing else. */
      .legal__lead {
        font-family: var(--label);
        font-weight: 700;
        font-size: 1.0625rem;
        padding-bottom: var(--s3);
        border-bottom: 3px solid var(--ink);
      }
      .legal__list {
        margin: 0 0 var(--s4);
        padding-left: var(--s5);
        max-width: 62ch;
      }
      .legal__list li {
        font-size: 0.9375rem;
        margin-bottom: var(--s2);
      }
    `,
  ],
})
export class DatenschutzComponent {}
