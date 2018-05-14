# rust

`rust/*` contains the rust logic, wrapped in calls to [wasm-bindgen](https://github.com/rustwasm/wasm-bindgen) to provide a js api

the files there are compiled to `js/assets/built-wasm/` as `wasm` + `js`

# js

`js/*` handles rendering the data created by the rust backend

The `wasm` + `js` apis are loaded into the main application [typescript](https://www.typescriptlang.org/) via async imports `await import(.*[wasm|js])'`
