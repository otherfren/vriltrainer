import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { AppComponent } from './app.component';
import { PlayerService } from './core/player.service';

describe('AppComponent', () => {
  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [AppComponent],
      providers: [provideRouter([])],
    }).compileComponents();
  });

  it('creates', () => {
    expect(TestBed.createComponent(AppComponent).componentInstance).toBeTruthy();
  });

  // The access key is the save file. Keeping it out of the page until it is asked for is what
  // makes the site safe to stream or screenshot (D9, D21), so it is worth a test.
  it('never renders the key on the common path', () => {
    const fixture = TestBed.createComponent(AppComponent);
    fixture.detectChanges();

    const secret = fixture.componentInstance.accessKey.split('#t=')[1];
    const bar = (fixture.nativeElement as HTMLElement).querySelector('.hud');
    expect(bar?.textContent).not.toContain(secret);
  });

  // Stronger than it was: the dialog itself opens covered, so the key is not anywhere in the
  // document until it has been asked for a second time, inside the dialog.
  it('keeps the key out of the document until it is revealed', () => {
    const fixture = TestBed.createComponent(AppComponent);
    fixture.detectChanges();

    const root = fixture.nativeElement as HTMLElement;
    const dialog = root.querySelector('dialog');

    expect(dialog?.open).toBeFalsy();
    expect(root.textContent).not.toContain(fixture.componentInstance.accessKey);
    expect(dialog?.textContent).toContain(fixture.componentInstance.maskedKey);
  });

  it('reveals it on request, and covers it again on close', () => {
    const fixture = TestBed.createComponent(AppComponent);
    const { componentInstance: app } = fixture;
    fixture.detectChanges();

    const dialog = (fixture.nativeElement as HTMLElement).querySelector('dialog');

    app.toggleReveal();
    fixture.detectChanges();
    expect(dialog?.textContent).toContain(app.accessKey);

    // Closing is not just hiding: reopening must not hand back a dialog that is already open.
    app.closeKey();
    fixture.detectChanges();
    expect(dialog?.textContent).not.toContain(app.accessKey);
  });

  // The bar names the account so it is obvious which one you are about to copy, and the whole
  // panel is the control — there is only one thing to do with a login you cannot read.
  it('names the account in the bar, as a single button', () => {
    const fixture = TestBed.createComponent(AppComponent);
    const root = fixture.nativeElement as HTMLElement;
    fixture.detectChanges();

    expect(root.querySelector('.login')).toBeNull();

    TestBed.inject(PlayerService).name.set('Testperson');
    fixture.detectChanges();

    const login = root.querySelector('.login');
    expect(login?.tagName).toBe('BUTTON');
    expect(login?.textContent).toContain('Testperson');
    expect(login?.querySelectorAll('button').length).toBe(0);
  });
});
