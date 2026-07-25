import { Component, ElementRef, inject, viewChild } from '@angular/core';
import { RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';
import { StatusPanelComponent } from './core/status-panel.component';
import { SceneComponent } from './core/scene.component';
import { ApiService } from './core/api.service';
import { PlayerService } from './core/player.service';

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
  /** Shown in the HUD, because a demo trial that looked real would be the one dishonest
   *  thing on a site whose entire promise is that you can check it yourself. */
  readonly demoMode = inject(ApiService).demoMode;
  readonly player = inject(PlayerService);

  readonly accessKey = 'https://vriltrainer.de/#t=8f2c41a09b7e5d63a1c8ff02e94b7d15';
  readonly maskedKey = maskToken(this.accessKey);

  private readonly dialog = viewChild.required<ElementRef<HTMLDialogElement>>('keyDialog');

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
    this.dialog().nativeElement.showModal();
  }

  closeKey(): void {
    this.revealed = false;
    this.dialog().nativeElement.close();
  }

  toggleReveal(): void {
    this.revealed = !this.revealed;
  }

  /** A click on the backdrop lands on the dialog element itself, never on its contents. */
  onDialogClick(event: MouseEvent): void {
    if (event.target === this.dialog().nativeElement) this.closeKey();
  }

  copyKey(): void {
    // Copying never requires revealing: the common path must not render the secret at all.
    navigator.clipboard?.writeText(this.accessKey);
    this.copied = true;
    setTimeout(() => (this.copied = false), 2000);
  }
}
