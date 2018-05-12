const path = require("path");
const HtmlWebpackPlugin = require("html-webpack-plugin");

module.exports = {
  entry: "./client/bootstrap.js",
  output: {
    path: path.resolve(__dirname, "dist"),
    filename: "main.js"
  },
  resolve: {
    extensions: [".ts", ".js", ".wasm"]
  },
  module: {
    rules: [{ test: /\.ts$/, loader: "ts-loader" }]
  },
  node: {
    fs: "empty",
    module: "empty"
  },
  plugins: [new HtmlWebpackPlugin()],
  mode: "development"
};
