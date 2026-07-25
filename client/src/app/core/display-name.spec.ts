import { checkDisplayName, normaliseDisplayName } from './display-name';

describe('checkDisplayName', () => {
  const ok = (n: string) => expect(checkDisplayName(n).ok).withContext(n).toBeTrue();
  const no = (n: string) => expect(checkDisplayName(n).ok).withContext(n).toBeFalse();

  it('accepts ordinary names', () => {
    ok('otherfren');
    ok('ganzfeld_enjoyer');
    ok('Monroe Institut');
    ok('Zoë');
    ok('remote-viewer-42');
    ok('Müller88');
    // Surrounding whitespace is trimmed rather than refused; only inner edges are an error.
    ok('otherfren ');
  });

  it('refuses the shapeless ones', () => {
    no('');
    no('  ');
    no('ab');
    no('x'.repeat(21));
    no('1488');
    no('_otherfren');
    no('otherfren_');
    no('-otherfren');
    no('aaaaa');
    no('a1');
    no('vril<script>');
    no('www.beispiel.de');
  });

  it('refuses names the site uses for itself', () => {
    no('admin');
    no('Moderator');
    no('vriltrainer');
    no('SYSTEM');
  });

  // The board is public and the subject matter attracts exactly this, so it is worth asserting
  // rather than assuming — including through the leet folding.
  it('refuses hate terms, spelled straight or in leet', () => {
    no('SiegHeil88');
    no('h1tl3r');
    no('Heil Hitler');
    no('white power');
    no('NSDAP_fan');
  });

  it('refuses vulgarity', () => {
    no('Hurensohn');
    no('fuckyou');
    no('W1chser');
  });

  it('normalises whitespace before storing', () => {
    expect(normaliseDisplayName('  Monroe   Institut ')).toEqual('Monroe Institut');
  });

  it('explains itself', () => {
    expect(checkDisplayName('ab').message).toContain('3');
    expect(checkDisplayName('admin').message).toBeTruthy();
  });
});
