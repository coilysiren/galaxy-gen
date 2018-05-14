`client/*` handles rendering the data created by the rust backend

`setup.js` handles loading the js binds + wasm binary into `main.ts`. `setup.js` has to be a `js` file rather than `ts` because of a quirk of how `typescript@^2.8` compiles `async import(...`

`main.ts` is [typescript](https://www.typescriptlang.org/) because typescript is good and pure :sparkles:

`tests/*` are [mocha](https://mochajs.org/) tests in a [karma](https://karma-runner.github.io/) runner, they run a chrome browser and execute the `wasm` + `js` there.
