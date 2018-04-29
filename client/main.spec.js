import { mainScriptWithWasm } from "./bootstrap";

describe("main generation script", () => {

  beforeAll(async () => {
    const MainScript = await mainScriptWithWasm;
  })

  it("executes tests", () => {
    expect(true).toBeTruthy();
  });

});
