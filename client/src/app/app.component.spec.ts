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

  // The access key is the save file. Masking it by default is what keeps the page safe to
  // stream or screenshot (D9, D21), so it is worth a test rather than a comment.
  it('masks the access key until it is asked for', () => {
    const app = TestBed.createComponent(AppComponent).componentInstance;

    expect(app.maskedKey).not.toContain(app.accessKey.split('#t=')[1]);
    expect(app.maskedKey).toContain('•');

    app.toggleKey();
    expect(app.maskedKey).toEqual(app.accessKey);
  });

  it('never renders the key into the page on the common path', () => {
    const fixture = TestBed.createComponent(AppComponent);
    fixture.detectChanges();

    const secret = fixture.componentInstance.accessKey.split('#t=')[1];
    expect((fixture.nativeElement as HTMLElement).textContent).not.toContain(secret);
  });
});
