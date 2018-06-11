/**
 * awaits wasm `${SCRIPT}_bg.wasm` compilation
 * then passes it into the main script
 */

import { MainScript } from "./main";

export async function setupMainScript() {
  return new MainScript(
    await import("./../../pkg/galaxy_gen_backend"),
    // @ts-ignore: ignore `module not found` for the wasm file
    await import("./../../pkg/galaxy_gen_backend_bg.wasm")
  );
}
