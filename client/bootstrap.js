import { MainScript } from "./main";

const mainScriptWithWasm = import("./assets/built-wasm/galaxy_gen")
  .then((rust) => new MainScript(rust));
