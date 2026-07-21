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
    // Bake SENTRY_DSN at build time; browser has no later env hook.
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
  snapshot: {
    // Default managedPaths treat all of node_modules as immutable, which
    // serves a stale WASM module after a cargo-watch rebuild of pkg/.
    // Un-manage only the symlinked wasm-pack output.
    managedPaths: [/^(.+?[\\/]node_modules[\\/])(?!galaxy_gen_backend)/],
  },
  devServer: {
    hot: true,
    liveReload: true,
    // 8081, not 8080: a homebrew nginx login service holds 8080 on this
    // host and grabs it back whenever the dev server restarts.
    port: 8081,
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
