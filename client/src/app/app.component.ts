import { Component, ElementRef, computed, inject, viewChild } from '@angular/core';
import { RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';
import { StatusPanelComponent } from './core/status-panel.component';
import { SceneComponent } from './core/scene.component';
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

  copied = false;
  revealed = false;

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

  copyKey(): void {
    // Copying never requires revealing: the common path must not render the secret at all.
    navigator.clipboard?.writeText(this.accessKey());
    this.copied = true;
    setTimeout(() => (this.copied = false), 2000);
  }
}
