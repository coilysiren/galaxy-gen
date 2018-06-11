# galaxyGen

[![Build Status](https://travis-ci.com/lynncyrin/galaxy-gen.svg?branch=main)](https://travis-ci.com/lynncyrin/galaxy-gen)

`{ rust => wasm => js }` galaxy generation simulation

[previous verison](https://github.com/lynncyrin/galaxySim), written in python

## commands

- `$ make install`
- `$ make dev`
- see [makefile](makefile) for others

## infrastructure

- `./src/rust/` is the rust backend, with [wasm-bindgen](https://github.com/rustwasm/wasm-bindgen) decorators. the first build stage runs rust tests via `cargo test`
- `./pkg/` is created via [wasm-pack](https://github.com/ashleygwilliams/wasm-pack), which compiles the rust code to `wasm` + `js` + `typescript`
- `./src/js/` imports `./pkg/` and runs js tests via `npm test`
- `./dist/` is created via [webpack](https://webpack.js.org/), which compiles everything mentioned above + [angular](http://angular.io/)
- http://galaxygen.lynncyrin.me is updated via [heroku](https://www.heroku.com/) with the compiled code

Note: the compiled folders aren't present on the default branch. Go to [{ branch: deploy }](https://github.com/lynncyrin/galaxy-gen/tree/deploy) to view them.
