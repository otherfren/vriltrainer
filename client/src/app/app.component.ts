import { Component, ElementRef, LOCALE_ID, computed, inject, viewChild } from '@angular/core';
import { Meta, Title } from '@angular/platform-browser';
import { RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';
import { StatusPanelComponent } from './core/status-panel.component';
import { SceneComponent } from './core/scene.component';
import { ApiService } from './core/api.service';
import { PlayerService } from './core/player.service';
import { SessionService } from './core/session.service';

/**
 * Everything after `#t=` becomes dots. The origin stays legible, so what you are looking at is
 * still recognisably your own link before you decide to uncover it, and the string keeps its
 * length either way — revealing does not reflow the dialog.
 */
function maskToken(url: string): string {
  const at = url.indexOf('#t=');
  if (at < 0) return '•'.repeat(url.length);
  const head = url.slice(0, at + '#t='.length);
  return head + '•'.repeat(url.length - head.length);
}

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [RouterOutlet, RouterLink, RouterLinkActive, StatusPanelComponent, SceneComponent],
  templateUrl: './app.component.html',
  styleUrl: './app.component.scss',
})
export class AppComponent {
  readonly player = inject(PlayerService);
  readonly session = inject(SessionService);
  private readonly api = inject(ApiService);

  /**
   * Which side of the language switch is the one you are on.
   *
   * From `LOCALE_ID`, which the localized build sets, and not from the host: the two are meant to
   * agree (D10/D24), and if they ever do not, the bundle is what the visitor is actually reading.
   */
  readonly german = inject(LOCALE_ID).startsWith('de');

  constructor() {
    // Angular's localized build replaces the `lang` attribute in `index.html` and nothing else, so
    // the title and description in that file are the German source in both bundles. Setting them
    // here is the only place they can differ per locale without a second index file. The cost is
    // that the German title is on screen for the moment before bootstrap; the alternative was an
    // English domain whose browser tab said `öffentlicher Remote-Viewing-Test`.
    inject(Title).setTitle($localize`:@@meta.title:vriltrainer - öffentlicher Remote-Viewing-Test`);
    inject(Meta).updateTag({
      name: 'description',
      content: $localize`:@@meta.description:Ich rekrutiere psionische Assets um die Reptiloiden zu bekämpfen. Teste deine telepathischen Fähigkeiten!`,
    });
  }

  /**
   * The link that *is* the account, as this browser actually holds it.
   *
   * Read from the session rather than kept here, and empty until an account exists. The fragment
   * never reached a server on the way in, and this string leaves the page only through the
   * clipboard (D9, FR-006).
   */
  readonly accessKey = computed(() => this.session.accessLink() ?? '');
  readonly maskedKey = computed(() => maskToken(this.accessKey()));

  /**
   * Optional, unlike before: the dialog is inside the same `@if` as the button that opens it, so
   * for a visitor without an account it is not in the document at all. A required query would
   * throw on that entirely ordinary first page load.
   */
  private readonly dialog = viewChild<ElementRef<HTMLDialogElement>>('keyDialog');
  private readonly logoutDialog = viewChild<ElementRef<HTMLDialogElement>>('logoutDialog');

  copied = false;
  revealed = false;

  /**
   * Two strings the template can only reach through an expression — a fallback in a `??` chain and
   * an attribute bound conditionally. `$localize` is how those get into the catalogue at all; a
   * quoted string inside a template expression is code and the extractor walks straight past it.
   */
  readonly fallbackAccountLabel = $localize`:@@login.fallbackName:Konto`;
  readonly maskedKeyLabel = $localize`:@@keydlg.maskedLabel:Login verdeckt`;

  /**
   * Revealing happens in a modal rather than in the bar, because the bar is 15rem wide and a
   * key shown as `…` is not shown at all. Here there is room for it whole, selectable and out
   * of the way of anything you might be screenshotting.
   *
   * It opens masked. Opening the panel and exposing the secret are two different acts, and the
   * usual reason to open it — copying — does not need the secret on screen at all.
   */
  openKey(): void {
    this.copied = false;
    this.revealed = false;
    this.dialog()?.nativeElement.showModal();
  }

  closeKey(): void {
    this.revealed = false;
    this.dialog()?.nativeElement.close();
  }

  toggleReveal(): void {
    this.revealed = !this.revealed;
  }

  /** A click on the backdrop lands on the dialog element itself, never on its contents. */
  onDialogClick(event: MouseEvent): void {
    if (event.target === this.dialog()?.nativeElement) this.closeKey();
  }

  /**
   * Crosses to the other domain carrying the session (D11, FR-031, T067).
   *
   * The two domains are separate origins, so `localStorage` does not travel: without this the
   * switch arrives as an anonymous first-time visitor, and the name gate then creates a *second*
   * account. One person would sit in the leaderboard and the aggregate twice, with their trials
   * split across both.
   *
   * What crosses is a handoff code, never the long-lived token — single use, thirty seconds, and
   * worthless once burnt, so a URL somebody is streaming gives nothing away.
   *
   * The modified clicks are left to the browser. Ctrl-click and middle-click mean "open a copy
   * over there", and a code is single-use: spending it on a tab the visitor is not looking at
   * would be worse than arriving anonymously.
   */
  async switchTo(origin: string, event: MouseEvent): Promise<void> {
    if (event.defaultPrevented || event.button !== 0) return;
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    if (!this.session.signedIn()) return;

    event.preventDefault();
    try {
      const code = await this.api.mintHandoff();
      location.assign(`${origin}/#h=${code}`);
    } catch {
      // The switch itself must not be what fails. Arriving without the session is the outcome
      // this feature improves on, not a broken state — and it is still the other language.
      location.assign(origin);
    }
  }

  copyKey(): void {
    // Copying never requires revealing: the common path must not render the secret at all.
    navigator.clipboard?.writeText(this.accessKey());
    this.copied = true;
    setTimeout(() => (this.copied = false), 2000);
  }

  /**
   * Signing out shows the link unmasked, which is the opposite of every other panel here.
   *
   * Everywhere else the secret starts covered because the usual reason to open the panel is not
   * to read it. Here it is: this is the last moment the link exists in this browser, and a
   * covered key behind one more click is how somebody confirms away an account they meant to
   * keep.
   */
  openLogout(): void {
    this.copied = false;
    this.logoutDialog()?.nativeElement.showModal();
  }

  cancelLogout(): void {
    this.logoutDialog()?.nativeElement.close();
  }

  confirmLogout(): void {
    this.logoutDialog()?.nativeElement.close();
    this.session.signOut();
  }

  /** Whether the erase step is showing its confirmation, and whether the call is in flight. */
  erasing = false;
  erasingNow = false;
  eraseFailed = false;

  /**
   * Deletes the display name and nothing else (FR-035).
   *
   * The session keeps its token — the account is still yours to play, it simply has no name any
   * more — so the stored record is rewritten rather than dropped, and the header falls through to
   * the public identifier the way it does for a browser that arrived by access link. The figures
   * are reloaded because the leaderboard row the panel links to now reads differently.
   */
  async eraseName(): Promise<void> {
    const account = this.session.account();
    this.erasingNow = true;
    this.eraseFailed = false;
    try {
      await this.api.eraseName();
      if (account) this.session.rememberAccount({ ...account, name: '' });
      void this.player.refresh();
      this.erasing = false;
    } catch {
      // Deliberately no detail: the one thing a visitor can do about it is try again, and the
      // status code of a DELETE they did not know they were making is not information to them.
      this.eraseFailed = true;
    } finally {
      this.erasingNow = false;
    }
  }

  onLogoutDialogClick(event: MouseEvent): void {
    if (event.target === this.logoutDialog()?.nativeElement) this.cancelLogout();
  }
}
