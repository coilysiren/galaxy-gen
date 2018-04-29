// Karma configuration
// Generated on Fri Feb 23 2018 18:18:19 GMT-0800 (PST)

module.exports = function (config) {
  config.set({
    basePath: "./../",
    plugins: [
      require('karma-jasmine'),
      require('karma-firefox-launcher'),
      require("karma-typescript"),
      require("karma-spec-reporter"),
    ],
    frameworks: [
      "jasmine",
      "karma-typescript",
    ],
    preprocessors: {
      "**/*.ts": "karma-typescript"
    },
    files: [
      'client/**/*.spec.js',
    ],
    reporters: [
      "spec",
    ],
    client: {
      captureConsole: true,
      clearConsole: false,
    },
    port: 9876,
    colors: true,
    logLevel: config.LOG_INFO,
    browsers: [
      "Firefox",
    ],
    concurrency: Infinity
  })
}
