import { TestBed } from '@angular/core/testing';
import { SessionService } from './session.service';

const TOKEN = '8f2c41a09b7e5d63a1c8ff02e94b7d15';

/**
 * D9's whole argument is that the token travels in a fragment, which is never transmitted, and
 * that it stops being in the address bar the moment it has been read. Both halves are one line of
 * code each and neither fails loudly, so both are tested.
 */
describe('SessionService', () => {
  beforeEach(() => {
    localStorage.clear();
    history.replaceState(null, '', location.pathname + location.search);
    TestBed.configureTestingModule({});
  });

  afterEach(() => {
    localStorage.clear();
    history.replaceState(null, '', location.pathname + location.search);
  });

  it('takes the token out of the fragment and clears the address bar', () => {
    location.hash = `#t=${TOKEN}`;

    const session = TestBed.inject(SessionService);

    expect(session.token()).toBe(TOKEN);
    expect(location.hash).toBe('');
    expect(location.href).not.toContain(TOKEN);
  });

  it('survives a reload', () => {
    TestBed.inject(SessionService).establish(TOKEN, { publicId: '7F3A9C', name: 'Testperson' });

    // A second injector is the closest thing to a reload: same storage, new service.
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({});
    const reloaded = TestBed.inject(SessionService);

    expect(reloaded.token()).toBe(TOKEN);
    expect(reloaded.account()?.name).toBe('Testperson');
  });

  // A different token is a different account. Keeping the old name would put one person's name in
  // the header over another person's trials.
  it('drops the account record when a different token arrives', () => {
    const session = TestBed.inject(SessionService);
    session.establish(TOKEN, { publicId: '7F3A9C', name: 'Testperson' });

    session.adopt('0000000000000000000000000000dead');

    expect(session.account()).toBeNull();
    expect(session.token()).toBe('0000000000000000000000000000dead');
  });

  it('keeps the account record when the same token is re-adopted', () => {
    const session = TestBed.inject(SessionService);
    session.establish(TOKEN, { publicId: '7F3A9C', name: 'Testperson' });

    session.adopt(TOKEN);

    expect(session.account()?.publicId).toBe('7F3A9C');
  });

  it('builds the access link against the origin actually being served', () => {
    const session = TestBed.inject(SessionService);
    expect(session.accessLink()).toBeNull();

    session.adopt(TOKEN);
    expect(session.accessLink()).toBe(`${location.origin}/#t=${TOKEN}`);
  });
});
