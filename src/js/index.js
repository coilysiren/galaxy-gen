import React from "react";
import * as ReactDOM from "react-dom/client";
import * as application from "./lib/application";

// A dependency graph that contains any wasm must all be imported asynchronously.
const galaxy = import("./lib/galaxy").catch((e) =>
  console.error("Error importing `main`:", e)
);

ReactDOM.createRoot(document.getElementById("root")).render(
  <application.Interface />
);
