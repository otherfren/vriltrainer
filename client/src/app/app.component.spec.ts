import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { AppComponent } from './app.component';
import { SessionService } from './core/session.service';

const TOKEN = '8f2c41a09b7e5d63a1c8ff02e94b7d15';

describe('AppComponent', () => {
  let session: SessionService;

  beforeEach(async () => {
    localStorage.clear();
    await TestBed.configureTestingModule({
      imports: [AppComponent],
      providers: [provideRouter([])],
    }).compileComponents();
    session = TestBed.inject(SessionService);
  });

  it('creates', () => {
    expect(TestBed.createComponent(AppComponent).componentInstance).toBeTruthy();
  });

  // Before there is an account there is no login to show, and — the part worth a test — no dialog
  // in the document that could hold one.
  it('shows nothing to open until there is a token', () => {
    const fixture = TestBed.createComponent(AppComponent);
    fixture.detectChanges();

    const root = fixture.nativeElement as HTMLElement;
    expect(root.querySelector('.login')).toBeNull();
    expect(root.querySelector('dialog')).toBeNull();
  });

  // The access key is the save file. Keeping it out of the page until it is asked for is what
  // makes the site safe to stream or screenshot (D9, D21), so it is worth a test.
  it('never renders the key on the common path', () => {
    session.establish(TOKEN, { publicId: '7F3A9C', name: 'Testperson' });
    const fixture = TestBed.createComponent(AppComponent);
    fixture.detectChanges();

    const bar = (fixture.nativeElement as HTMLElement).querySelector('.hud');
    expect(bar?.textContent).not.toContain(TOKEN);
  });

  // Stronger than it looks: the dialog itself opens covered, so the key is not anywhere in the
  // document until it has been asked for a second time, inside the dialog.
  it('keeps the key out of the document until it is revealed', () => {
    session.establish(TOKEN, { publicId: '7F3A9C', name: 'Testperson' });
    const fixture = TestBed.createComponent(AppComponent);
    fixture.detectChanges();

    const root = fixture.nativeElement as HTMLElement;
    const dialog = root.querySelector('dialog');

    expect(dialog?.open).toBeFalsy();
    expect(root.textContent).not.toContain(TOKEN);
    expect(dialog?.textContent).toContain(fixture.componentInstance.maskedKey());
  });

  it('reveals it on request, and covers it again on close', () => {
    session.establish(TOKEN, { publicId: '7F3A9C', name: 'Testperson' });
    const fixture = TestBed.createComponent(AppComponent);
    const app = fixture.componentInstance;
    fixture.detectChanges();

    const dialog = (fixture.nativeElement as HTMLElement).querySelector('dialog');

    app.toggleReveal();
    fixture.detectChanges();
    expect(dialog?.textContent).toContain(TOKEN);

    // Closing is not just hiding: reopening must not hand back a dialog that is already open.
    app.closeKey();
    fixture.detectChanges();
    expect(dialog?.textContent).not.toContain(TOKEN);
  });

  // The bar names the account so it is obvious which one you are about to copy, and the whole
  // panel is the control — there is only one thing to do with a login you cannot read.
  it('names the account in the bar, as a single button', () => {
    session.establish(TOKEN, { publicId: '7F3A9C', name: 'Testperson' });
    const fixture = TestBed.createComponent(AppComponent);
    const root = fixture.nativeElement as HTMLElement;
    fixture.detectChanges();

    const login = root.querySelector('.login');
    expect(login?.tagName).toBe('BUTTON');
    expect(login?.textContent).toContain('Testperson');
    expect(login?.querySelectorAll('button').length).toBe(0);
  });

  // An account adopted from an access link holds a token and no name: there is no endpoint that
  // hands the holder their own name back, so the bar falls through to the public identifier — and
  // the screen behind it must not fall through to the name gate, which would create a second
  // account and orphan the first (FR-005).
  it('still shows a login when only a token is known', () => {
    session.adopt(TOKEN);
    const fixture = TestBed.createComponent(AppComponent);
    fixture.detectChanges();

    const login = (fixture.nativeElement as HTMLElement).querySelector('.login');
    expect(login).not.toBeNull();
    expect(login?.textContent).not.toContain(TOKEN);
  });
});
