import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { AppComponent } from './app.component';

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

  it('reveals it only inside the dialog', () => {
    const fixture = TestBed.createComponent(AppComponent);
    fixture.detectChanges();

    const dialog = (fixture.nativeElement as HTMLElement).querySelector('dialog');
    expect(dialog?.open).toBeFalsy();
    expect(dialog?.textContent).toContain(fixture.componentInstance.accessKey);
  });
});
