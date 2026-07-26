import {
  ApplicationConfig,
  inject,
  provideAppInitializer,
  provideZoneChangeDetection,
} from '@angular/core';
import { provideRouter } from '@angular/router';

import { ApiService } from './core/api.service';
import { SessionService } from './core/session.service';
import { routes } from './app.routes';

/**
 * Burns a handoff code before the first screen is drawn (D11, FR-031).
 *
 * Awaited rather than fired and forgotten. The trial screen decides what to show from
 * `signedIn()`, so a redemption still in flight when it renders shows the name gate to somebody
 * who already has an account — and a name typed into that gate creates a *second* one, which is
 * the exact failure the handoff exists to prevent.
 *
 * The code lives thirty seconds, so the wait is one round trip and no more.
 *
 * A failure is swallowed on purpose. An expired or already-burnt code is not something the
 * visitor can act on, and the honest outcome is the session this browser already had: they arrive
 * as whoever they were on this domain, which is what would have happened without the switch.
 */
function redeemPendingHandoff(): Promise<void> {
  const session = inject(SessionService);
  const api = inject(ApiService);

  const code = session.pendingHandoff();
  if (code === null) return Promise.resolve();

  return api.redeemHandoff(code).catch(() => undefined);
}

export const appConfig: ApplicationConfig = {
  providers: [
    provideZoneChangeDetection({ eventCoalescing: true }),
    provideRouter(routes),
    provideAppInitializer(redeemPendingHandoff),
  ],
};
