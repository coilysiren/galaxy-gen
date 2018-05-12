`client/*` handles rendering the data created by the rust backend

`bootstrap.js` handles loading the js binds + wasm binary into `main.ts`. `bootstrap.js` has to be a `js` file rather than `ts` because of a quirk of how `typescript@^2.8` compiles `async import(...`

`tests/*` are karma tests, they run a chrome browser and execute the `wasm` + `js` there.
