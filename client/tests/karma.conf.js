// Karma configuration
// Generated on Fri May 11 2018 21:05:49 GMT-0700 (PDT)

module.exports = function(config) {
  config.set({
    basePath: "./../../",
    browsers: ["Chrome"],
    frameworks: ["jasmine", "karma-typescript"],
    plugins: [
      require("karma-jasmine"),
      require("karma-chrome-launcher"),
      require("karma-spec-reporter"),
      require("karma-typescript")
    ],
    files: ["client/tests/*spec.ts"],
    preprocessors: {
      "./**/*.ts": "karma-typescript"
    },
    reporters: ["spec", "karma-typescript"],
    port: 9876,
    colors: true,
    logLevel: config.LOG_INFO,
    autoWatch: false,
    singleRun: true,
    concurrency: Infinity,
    karmaTypescriptConfig: {
      tsconfig: "client/tests/tsconfig.json"
    }
  });
};
