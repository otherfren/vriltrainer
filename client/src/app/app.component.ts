import { Component, inject } from '@angular/core';
import { RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';
import { VrilMeterComponent } from './core/vril-meter.component';
import { ApiService } from './core/api.service';

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [RouterOutlet, RouterLink, RouterLinkActive, VrilMeterComponent],
  templateUrl: './app.component.html',
  styleUrl: './app.component.scss',
})
export class AppComponent {
  /** Shown in the HUD, because a demo trial that looked real would be the one dishonest
   *  thing on a site whose entire promise is that you can check it yourself. */
  readonly demoMode = inject(ApiService).demoMode;

  /** Masked by default so the page stays safe to stream or screenshot (D9, D21). */
  keyRevealed = false;
  readonly accessKey = 'https://vriltrainer.de/#t=8f2c41a09b7e5d63a1c8ff02e94b7d15';

  get maskedKey(): string {
    return this.keyRevealed ? this.accessKey : 'https://vriltrainer.de/#t=' + '•'.repeat(32);
  }

  toggleKey(): void {
    this.keyRevealed = !this.keyRevealed;
    if (this.keyRevealed) {
      // Re-masks itself rather than staying open for the rest of the session.
      setTimeout(() => (this.keyRevealed = false), 12000);
    }
  }

  copyKey(): void {
    // Copying never reveals: the common path must not render the secret at all.
    navigator.clipboard?.writeText(this.accessKey);
  }
}
