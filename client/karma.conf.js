// Karma configuration
// Generated on Fri Feb 23 2018 18:18:19 GMT-0800 (PST)

module.exports = function (config) {
  config.set({
    plugins: [
      require('karma-jasmine'),
      require('karma-firefox-launcher'),
      require("karma-spec-reporter"),
    ],
    frameworks: [
      "jasmine",
    ],
    files: [
      './*.spec.js',
    ],
    reporters: [
      "spec",
    ],
    port: 9876,
    colors: true,
    logLevel: config.LOG_INFO,
    browsers: [
      "Firefox",
    ],
    concurrency: Infinity
  })
}
