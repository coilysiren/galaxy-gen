const path = require("path");
const HtmlWebpackPlugin = require("html-webpack-plugin");

module.exports = {
  entry: path.resolve(__dirname, "bootstrap.js"),
  output: {
    path: path.resolve(__dirname, "dist")
  },
  resolve: {
    extensions: [".ts", ".js", ".wasm", ".png"]
  },
  module: {
    rules: [{ test: /\.ts$/, loader: "ts-loader" }]
  },
  plugins: [
    new HtmlWebpackPlugin({
      template: path.resolve(__dirname, "index.html"),
      favicon: path.resolve(__dirname, "assets/favicon.png")
    })
  ],
  optimization: {
    splitChunks: {
      chunks: "all"
    }
  },
  mode: "development"
};
