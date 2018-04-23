import("./built-wasm/galaxy_gen").then((js) => {
  js.greet("Rust and WebAssembly?");
});
