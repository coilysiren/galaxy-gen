import React from "react";
import * as ReactDOM from "react-dom/client";
import * as Sentry from "@sentry/browser";
import * as application from "./lib/application";

// DSN is injected at build time via webpack DefinePlugin (see webpack.config.js).
// Absent in local dev unless SENTRY_DSN is exported before `npm run build` / `npm run dev`.
if (process.env.SENTRY_DSN) {
  Sentry.init({ dsn: process.env.SENTRY_DSN });
}

ReactDOM.createRoot(document.getElementById("root")).render(<application.Interface />);
