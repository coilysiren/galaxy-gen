import { MainScript } from "./../main.ts";
var assert = require("assert");

describe("galaxGen wasm", () => {
  it("runs tests", () => {
    assert.ok(true);
  });

  it("starts the main script", async () => {
    // mainScript = await setupMainScript();
    // expect(mainScript).toBeTruthy();
  });

  it("generates cells", async () => {
    // mainScript = await setupMainScript();
    // expect(mainScript.cells()).toBeTruthy();
  });
});

// async function setupMainScript() {
//   return new MainScript(
//     await import("./assets/built-wasm/galaxy_gen"),
//     // @ts-ignore: ignore `module not found` for the wasm file
//     await import("./assets/built-wasm/galaxy_gen_bg.wasm")
//   );
// }
