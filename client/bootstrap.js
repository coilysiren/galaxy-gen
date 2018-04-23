const jsWasmPromise = import("./assets/built-wasm/galaxy_gen");

jsWasmPromise.then((rust) => {
  console.log(rust.inform_logger("Rust and WebAssembly?"));
});
