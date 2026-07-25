import { Component, computed, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { RankDef, rankForPercentile, rankIcon } from '../core/ranks';

interface Row {
  rank: number;
  title: string;
  icon: string;
  name: string;
  publicId: string;
  wilson: number;
  completed: number;
  rate: number;
  z: number;
}

const NAMES = [
  'otherfren', 'ganzfeld_enjoyer', 'monroe_institut', 'stargate_9901', 'kein_name', 'vril_ya',
  'coordinate_hound', 'zenerkarte', 'ingo_swann_fan', 'sitzt_im_dunkeln', 'psi_oder_nicht',
  'aetherpost', 'nachtschicht', 'remote_viewer_42', 'hellsichtig_de', 'doppelblind',
  'nullhypothese', 'kein_effekt', 'wuenschelrute', 'dritte_auge_ag', 'ferngucker',
  'signal_im_rauschen', 'pendel_pilot', 'karten_leger', 'traumtagebuch', 'silberschnur',
  'astral_abwesend', 'orgon_ok', 'radiaesthet', 'zwischenraum', 'stille_post', 'katzenaugen',
  'nebelmaschine', 'tiefschlaf', 'wachtraum', 'kein_signal', 'suchbild', 'weitblick_ev',
  'fernfuehler', 'grauzone', 'schwingung9', 'aetherwelle', 'bildersucher', 'stichprobe',
  'konfidenz', 'binomial_bert', 'wilson_grenze', 'sigma_jaeger', 'ausreisser', 'langzeitreihe',
  'geduldsprobe', 'trefferzaehler', 'muenzwurf', 'wuerfelbecher', 'lostrommel', 'zufallszahl',
  'streuung', 'mittelwert', 'restrauschen', 'grundlinie', 'kalibriert', 'blindprobe',
  'kontrollgruppe', 'placebo_p', 'vorregistriert', 'praeregistrierung', 'datensatz', 'rohdaten',
  'nachrechner', 'pruefsumme', 'hashwert', 'seedgeber', 'wuerfelgott', 'letzter_platz',
];

@Component({
  selector: 'app-leaderboard',
  standalone: true,
  imports: [RouterLink],
  templateUrl: './leaderboard.component.html',
  styleUrl: './leaderboard.component.scss',
})
export class LeaderboardComponent {
  readonly eligible = 214;
  readonly required = 200;
  get ranksActive(): boolean {
    return this.eligible >= this.required;
  }

  readonly pageSize = 20;
  readonly page = signal(0);

  readonly rows: Row[] = LeaderboardComponent.demoRows();

  readonly pageCount = computed(() => Math.ceil(this.rows.length / this.pageSize));
  readonly pageRows = computed(() =>
    this.rows.slice(this.page() * this.pageSize, (this.page() + 1) * this.pageSize),
  );
  readonly pages = computed(() => Array.from({ length: this.pageCount() }, (_, i) => i));
  readonly firstShown = computed(() => this.page() * this.pageSize + 1);
  readonly lastShown = computed(() =>
    Math.min((this.page() + 1) * this.pageSize, this.rows.length),
  );

  go(p: number): void {
    this.page.set(Math.max(0, Math.min(this.pageCount() - 1, p)));
    scrollTo({ top: 0, behavior: 'smooth' });
  }

  icon(r: Row): string {
    return rankIcon(r.icon);
  }

  de(n: number, digits = 1): string {
    return n.toLocaleString('de-DE', {
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    });
  }

  /**
   * Demo board. Deterministic, so a screenshot of page 3 is the same page 3 tomorrow, and
   * monotone in the sort key, because a board that is not sorted by what it claims to sort by
   * is the sort of detail people notice.
   */
  private static demoRows(): Row[] {
    let seed = 0x7f4a21;
    const next = () => ((seed = (seed * 1664525 + 1013904223) >>> 0) / 4294967296);

    let wilson = 18.6;
    // Demo board: 74 rows standing in for a population of 720, so the shares land where the
    // ladder says they should instead of where a 74-row list would put them.
    const population = 720;
    return NAMES.map((name, i) => {
      const place = i + 1;
      const rank: RankDef = rankForPercentile(place / population);
      wilson -= 0.04 + next() * 0.07;
      const completed = 120 + Math.floor(next() * 1400);
      const rate = wilson + 1.4 + next() * 3.4;
      return {
        rank: place,
        title: rank.title,
        icon: rank.icon,
        name,
        publicId: (0x1000 + Math.floor(next() * 0xefff)).toString(16).toUpperCase(),
        wilson,
        completed,
        rate,
        z: 1.8 + next() * 2.6,
      };
    });
  }
}
