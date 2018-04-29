/**
 * awaits wasm `${SCRIPT}_bg.wasm` compilation
 * then passes it into the main script
 */

import { MainScript } from "./main";

import("./assets/built-wasm/galaxy_gen")
  .then((wasmBinds) => new MainScript(wasmBinds));
