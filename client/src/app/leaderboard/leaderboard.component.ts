import { Component } from '@angular/core';

interface Row {
  rank: number;
  title: string;
  name: string;
  publicId: string;
  wilson: number;
  completed: number;
  rate: number;
  z: number;
}

@Component({
  selector: 'app-leaderboard',
  standalone: true,
  templateUrl: './leaderboard.component.html',
  styleUrl: './leaderboard.component.scss',
})
export class LeaderboardComponent {
  readonly eligible = 214;
  readonly required = 200;
  get ranksActive(): boolean {
    return this.eligible >= this.required;
  }

  /** Positions, not thresholds. The supply of each rank is fixed however lucky anyone gets. */
  readonly ladder = [
    { title: 'Insektoider Archont', span: '1–3' },
    { title: 'Reptiloidenarchont', span: '4–10' },
    { title: 'Grey Alien', span: '11–30' },
    { title: 'Flugscheibenpilot', span: '31–80' },
    { title: 'Psionic Asset', span: '81–200' },
    { title: 'Normie', span: 'darunter' },
    { title: 'Kartoffel', span: 'deutlich unter dem Zufall' },
  ];

  de(n: number, digits = 1): string {
    return n.toLocaleString('de-DE', { minimumFractionDigits: digits, maximumFractionDigits: digits });
  }

  readonly rows: Row[] = [
    { rank: 1, title: 'Insektoider Archont', name: 'otherfren', publicId: 'K7QF', wilson: 18.1, completed: 430, rate: 21.2, z: 3.9 },
    { rank: 2, title: 'Insektoider Archont', name: 'ganzfeld_enjoyer', publicId: '2XM9', wilson: 17.4, completed: 612, rate: 19.8, z: 3.6 },
    { rank: 3, title: 'Insektoider Archont', name: 'monroe_institut', publicId: 'B4TT', wilson: 16.9, completed: 388, rate: 20.1, z: 3.2 },
    { rank: 4, title: 'Reptiloidenarchont', name: 'stargate_9901', publicId: 'QQ1W', wilson: 15.8, completed: 1204, rate: 17.2, z: 4.1 },
    { rank: 5, title: 'Reptiloidenarchont', name: 'kein_name', publicId: 'ZR03', wilson: 15.2, completed: 297, rate: 19.5, z: 2.8 },
    { rank: 6, title: 'Reptiloidenarchont', name: 'vril_ya', publicId: '7HND', wilson: 14.9, completed: 845, rate: 16.8, z: 3.3 },
  ];
}
