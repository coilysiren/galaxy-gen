const path = require("path");
const HtmlWebpackPlugin = require("html-webpack-plugin");
const CopyWebpackPlugin = require("copy-webpack-plugin");

module.exports = {
  entry: path.resolve(__dirname, "bootstrap.js"),
  output: {
    path: path.resolve(__dirname, "dist")
  },
  resolve: {
    extensions: [".ts", ".js", ".wasm", ".png"]
  },
  module: {
    rules: [
      { test: /\.ts$/, loader: "ts-loader" },
      { test: /\.png$/, loader: "file-loader" }
    ]
  },
  plugins: [
    new HtmlWebpackPlugin({
      template: path.resolve(__dirname, "index.html")
    })
  ],
  optimization: {
    splitChunks: {
      chunks: "all"
    }
  },
  mode: "development"
};
