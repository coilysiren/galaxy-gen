import React from "react";
import * as ReactDOM from "react-dom/client";
import App from "./lib/interface";

// A dependency graph that contains any wasm must all be imported asynchronously.
import("./lib/galaxy").catch((e) =>
  console.error("Error importing `main`:", e)
);

ReactDOM.createRoot(document.getElementById("root")).render(<App />);
