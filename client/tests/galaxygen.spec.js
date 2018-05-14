import { setupMainScript } from "./../setup";
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
