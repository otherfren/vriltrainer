import { Routes } from '@angular/router';

export const routes: Routes = [
  { path: '', pathMatch: 'full', redirectTo: 'trial' },
  {
    path: 'trial',
    loadComponent: () => import('./trial/trial.component').then((m) => m.TrialComponent),
  },
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
    path: 'impressum',
    loadComponent: () => import('./legal/impressum.component').then((m) => m.ImpressumComponent),
  },
  { path: '**', redirectTo: 'trial' },
];
