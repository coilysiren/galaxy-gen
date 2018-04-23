const path = require("path");

module.exports = {
  entry: "./client/bootstrap.js",
  output: {
    path: path.resolve(__dirname, "dist"),
    filename: "client/bootstrap.js",
  },
  mode: "development"
};