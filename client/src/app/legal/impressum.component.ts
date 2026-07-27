import { Component } from '@angular/core';

/**
 * § 5 DDG. Content taken from the operator's existing Impressum at konkin.io, minus the
 * sections that are specific to that project — its AI Act statement is about payment
 * middleware and its licence is Apache-2.0, neither of which is true here.
 */
@Component({
  selector: 'app-impressum',
  standalone: true,
  template: `
    <section class="shell legal">
      <p class="eyebrow" i18n="@@legal.eyebrow">Angaben gemäß § 5 DDG</p>
      <h1 i18n="@@legal.heading">Impressum</h1>

      <div class="legal__card panel">
        <h2 i18n="@@legal.responsible">Verantwortlich</h2>
        <p i18n="@@legal.address">
          Peter Geschel<br />
          Privatperson, nicht gewerblich<br />
          c/o IP-Management #8514<br />
          Ludwig-Erhard-Straße 18<br />
          20459 Hamburg<br />
          Deutschland
        </p>

        <h2 i18n="@@legal.contact">Kontakt</h2>
        <p i18n="@@legal.contactBlock">
          E-Mail: <a href="mailto:fren&#64;kek.to">fren&#64;kek.to</a><br />
          Telefon: +49 (0) 172 66 84 586<br />
          X.com: <a href="https://x.com/otherfren">&#64;otherfren</a>
        </p>

        <h2 i18n="@@legal.disclaimer">Haftungsausschluss</h2>

        <h3 i18n="@@legal.content.h">Haftung für Inhalte</h3>
        <p i18n="@@legal.content.p">
          Die Inhalte dieser Seiten wurden mit größter Sorgfalt erstellt. Für die Richtigkeit,
          Vollständigkeit und Aktualität der Inhalte können wir jedoch keine Gewähr übernehmen. Als
          Diensteanbieter sind wir gemäß § 7 Abs. 1 DDG für eigene Inhalte auf diesen Seiten nach
          den allgemeinen Gesetzen verantwortlich. Nach §§ 8 bis 10 DDG sind wir als
          Diensteanbieter jedoch nicht verpflichtet, übermittelte oder gespeicherte fremde
          Informationen zu überwachen oder nach Umständen zu forschen, die auf eine rechtswidrige
          Tätigkeit hinweisen.
        </p>

        <h3 i18n="@@legal.links.h">Haftung für Links</h3>
        <p i18n="@@legal.links.p">
          Unser Angebot enthält Links zu externen Webseiten Dritter, auf deren Inhalte wir keinen
          Einfluss haben. Deshalb können wir für diese fremden Inhalte auch keine Gewähr
          übernehmen. Für die Inhalte der verlinkten Seiten ist stets der jeweilige Anbieter oder
          Betreiber der Seiten verantwortlich.
        </p>

        <h3 i18n="@@legal.about.h">Zum Inhalt dieser Seite</h3>
        <p i18n="@@legal.about.p">
          vriltrainer ist ein öffentliches Forced-Choice-Experiment zum Remote Viewing. Die
          erwartete Trefferquote ist 12,5 % - reines Raten. Ränge, Titel und Abbildungen sind
          Satire auf die einschlägige Mythologie und keine Behauptung über die Wirklichkeit.
        </p>

        <h3 i18n="@@legal.licence.h">Urheberrecht und Lizenz</h3>
        <p i18n="@@legal.licence.p">
          Der Quellcode steht unter der
          <a href="https://github.com/otherfren/vriltrainer/blob/master/LICENSE">
            AGPL-3.0-or-later </a
          >. Wer eine veränderte Fassung betreibt, muss seine Änderungen veröffentlichen - für
          einen Dienst, dessen ganzes Versprechen Nachprüfbarkeit ist, ist das keine Formalie.
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
      .legal__card h3 {
        font-family: var(--label);
        font-weight: 800;
        font-size: 0.8125rem;
        letter-spacing: 0.12em;
        text-transform: uppercase;
        color: var(--pop-ink);
        margin: var(--s4) 0 var(--s2);
      }
      .legal__card p {
        font-size: 0.9375rem;
        max-width: 62ch;
      }
    `,
  ],
})
export class ImpressumComponent {}
