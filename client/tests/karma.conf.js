// Karma configuration
// Generated on Fri May 11 2018 21:05:49 GMT-0700 (PDT)

module.exports = function(config) {
  config.set({
    basePath: "./../",
    browsers: ["Chrome"],
    frameworks: ["jasmine"],
    plugins: [
      require("karma-jasmine"),
      require("karma-chrome-launcher"),
      require("karma-spec-reporter")
    ],
    files: ["client/tests/*spec.+(js|ts)"],
    reporters: ["spec"],
    port: 9876,
    colors: true,
    logLevel: config.LOG_INFO,
    autoWatch: false,
    singleRun: true,
    concurrency: Infinity
  });
};
