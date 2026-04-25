const CopyWebpackPlugin = require("copy-webpack-plugin");
const webpack = require("webpack");
const path = require("path");

module.exports = {
  entry: "./src/js/index.js",
  experiments: {
    asyncWebAssembly: true,
  },
  output: {
    path: path.resolve(__dirname, "dist"),
    filename: "index.js",
  },
  resolve: {
    extensions: [".ts", ".tsx", ".js", ".wasm", ".css"],
    // Follow symlinks so edits to the wasm-pack output under pkg/
    // (linked into node_modules/galaxy_gen_backend) trigger rebuilds.
    symlinks: false,
  },
  module: {
    rules: [
      {
        test: /\.(js|jsx|ts|tsx)$/,
        loader: "babel-loader",
        exclude: /node_modules/,
      },
      {
        test: /\.(css)$/,
        use: [{ loader: "style-loader" }, { loader: "css-loader" }, { loader: "postcss-loader" }],
      },
    ],
  },
  plugins: [
    new CopyWebpackPlugin({
      patterns: [
        { from: "src/js/index.html" },
        { from: "src/js/favicon.svg" },
      ],
    }),
    // Bake SENTRY_DSN into the bundle at build time. The browser is the
    // runtime, so there is no later env-injection hook. Falls back to an
    // empty string so the `if (process.env.SENTRY_DSN)` guard in index.js
    // short-circuits cleanly when the var is unset (local dev).
    new webpack.DefinePlugin({
      "process.env.SENTRY_DSN": JSON.stringify(process.env.SENTRY_DSN || ""),
    }),
  ],
  mode: "development",
  devtool: "eval-cheap-module-source-map",
  watchOptions: {
    ignored: ["**/node_modules/**", "!**/node_modules/galaxy_gen_backend/**"],
    aggregateTimeout: 200,
  },
  devServer: {
    hot: true,
    liveReload: true,
    port: 8080,
    host: "127.0.0.1",
    allowedHosts: "all",
    static: {
      directory: path.resolve(__dirname, "src/js"),
      watch: true,
    },
    // Watch the wasm-pack output so a `cargo watch`-driven rebuild
    // triggers a page reload automatically.
    watchFiles: ["pkg/**/*"],
    client: {
      overlay: { errors: true, warnings: false },
    },
  },
};
