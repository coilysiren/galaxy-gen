jsWasmPromise = import("./assets/built-wasm/galaxy_gen");

jsWasmPromise.then((js) => {
  js.greet("Rust and WebAssembly?");
});
