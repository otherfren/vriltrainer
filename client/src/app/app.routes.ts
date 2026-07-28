import { Routes } from '@angular/router';

// `/` is the trial screen, not a redirect to it. A cold visitor's first address bar should say
// the site they typed, and the screen they land on is already the combined landing-and-name gate
// (SC-001, FR-001) — bouncing them to /trial before they have an account named the thing they had
// not done yet.
const trial = () => import('./trial/trial.component').then((m) => m.TrialComponent);

export const routes: Routes = [
  { path: '', pathMatch: 'full', loadComponent: trial },
  // Kept as an alias: links to /trial were shared before `/` served this screen.
  { path: 'trial', loadComponent: trial },
  {
    path: 'statistik',
    loadComponent: () => import('./stats/stats.component').then((m) => m.StatsComponent),
  },
  {
    path: 'rangliste',
    loadComponent: () =>
      import('./leaderboard/leaderboard.component').then((m) => m.LeaderboardComponent),
  },
  {
    path: 'datenschutz',
    loadComponent: () => import('./legal/datenschutz.component').then((m) => m.DatenschutzComponent),
  },
  {
    path: 'impressum',
    loadComponent: () => import('./legal/impressum.component').then((m) => m.ImpressumComponent),
  },
  // Anything unrecognised lands on the trial screen at its canonical address rather than at the
  // alias, so a mistyped path does not leave the visitor somewhere the navigation does not mark.
  { path: '**', redirectTo: '' },
];
