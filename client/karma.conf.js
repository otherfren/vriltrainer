// Karma, named explicitly rather than left to the Angular builder's generated default.
//
// The generated one launches `Chrome`, which needs a display. That is why the client suite had
// never executed anywhere: it could not run on the server the site is built on and there was no
// configuration for a machine without a browser window either.
//
// `ChromeHeadlessNoSandbox` is the launcher CI uses. The sandbox is disabled because the runner is
// already a container — Chrome's own sandbox needs privileges a container does not grant, and the
// alternative is a suite that does not run, which is the state this file exists to end.
module.exports = function (config) {
  config.set({
    basePath: '',
    frameworks: ['jasmine', '@angular-devkit/build-angular'],
    plugins: [
      require('karma-jasmine'),
      require('karma-chrome-launcher'),
      require('karma-jasmine-html-reporter'),
      require('karma-coverage'),
      require('@angular-devkit/build-angular/plugins/karma'),
    ],
    client: {
      jasmine: {},
      // The browser log belongs in the terminal that ran the suite. A failure whose only trace is
      // in a headless browser's console is a failure nobody sees.
      captureConsole: true,
    },
    reporters: ['progress', 'kjhtml'],
    browsers: ['Chrome'],
    customLaunchers: {
      ChromeHeadlessNoSandbox: {
        base: 'ChromeHeadless',
        flags: [
          '--no-sandbox',
          '--disable-gpu',
          // Containers give /dev/shm 64 MB by default, which Chrome runs out of and dies in a way
          // that reads as a flaky test rather than as a memory limit.
          '--disable-dev-shm-usage',
        ],
      },
    },
    // CI runs `npm run test:ci`, which adds --watch=false; a bare `ng test` still watches, because
    // that is what it is for while somebody is writing a test.
    restartOnFileChange: true,
  });
};
