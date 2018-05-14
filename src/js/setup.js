/**
 * awaits wasm `${SCRIPT}_bg.wasm` compilation
 * then passes it into the main script
 */

import { MainScript } from "./main";

export async function setupMainScript() {
  return new MainScript(
    await import("./assets/built-wasm/galaxy_gen"),
    // @ts-ignore: ignore `module not found` for the wasm file
    await import("./assets/built-wasm/galaxy_gen_bg.wasm")
  );
}
