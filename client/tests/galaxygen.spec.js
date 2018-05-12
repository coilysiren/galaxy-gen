import { MainScript } from "./../main.ts";
import * as assert from "assert";

describe("galaxGen wasm", () => {
  it("runs tests", () => {
    assert.ok(true);
  });

  it("starts the main script without an error", async () => {
    const mainScript = await setupMainScript();
    assert.ok(mainScript);
  });

  it("generates cells", async () => {
    const mainScript = await setupMainScript();
    mainScript.generateData(10);
    assert.ok(mainScript.cells());
  });

  it("requires data generation to get cells", async () => {
    const mainScript = await setupMainScript();
    assert.throws(() => {
      mainScript.cells();
    });
  });
});

async function setupMainScript() {
  return new MainScript(
    await import("./../assets/built-wasm/galaxy_gen"),
    // @ts-ignore: ignore `module not found` for the wasm file
    await import("./../assets/built-wasm/galaxy_gen_bg.wasm")
  );
}
