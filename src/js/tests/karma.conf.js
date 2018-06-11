// Karma configuration
// Generated on Fri May 11 2018 21:05:49 GMT-0700 (PDT)

webpackConfig = require("./../webpack.config");

module.exports = function(config) {
  delete webpackConfig.optimization;
  config.set({
    basePath: "./../",
    browsers: ["ChromeHeadless"],
    frameworks: ["mocha"],
    files: ["tests/*spec.js"],
    preprocessors: {
      "./**/*.js": ["webpack"],
      "./**/*.ts": ["webpack"]
    },
    reporters: ["spec"],
    port: 9876,
    colors: true,
    logLevel: config.LOG_INFO,
    autoWatch: true,
    singleRun: false,
    concurrency: Infinity,
    karmaTypescriptConfig: {
      tsconfig: "tests/tsconfig.json"
    },
    webpack: webpackConfig
  });
};
