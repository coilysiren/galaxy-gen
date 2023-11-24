# galaxy-gen

`{ rust => wasm => js }` galaxy generation simulation

[previous verison](https://github.com/coilysiren/galaxySim), written in python

## commands

- `$ make install`
- `$ make dev`
- see [makefile](makefile) for others

## infrastructure

- `./src/rust/` is the rust backend, with [wasm-bindgen](https://github.com/rustwasm/wasm-bindgen) decorators. the first build stage runs rust tests via `cargo test`
- `./pkg/` is created via [wasm-pack](https://github.com/ashleygwilliams/wasm-pack), which compiles the rust code to `wasm` + `js` + `typescript`
- `./src/js/` imports `./pkg/` and runs js tests via `npm test`
- `./dist/` is created via [webpack](https://webpack.js.org/), which compiles everything mentioned above + [angular](http://angular.io/)
- http://galaxygen.coilysiren.me is updated via [heroku](https://www.heroku.com/) with the compiled code

Note: the compiled folders aren't present on the default branch. Go to [{ branch: deploy }](https://github.com/coilysiren/galaxy-gen/tree/deploy) to view them.

## code

## rust

`rust/*` contains the rust logic, wrapped in calls to [wasm-bindgen](https://github.com/rustwasm/wasm-bindgen) to provide a js api

the files there are compiled to `galaxy_gen_backend` as `wasm` + `js`

## rust files

`galaxy.rs` represents the proxy of a real life `Galaxy`, like a [spiral](https://en.wikipedia.org/wiki/Spiral_galaxy) or [ring](https://en.wikipedia.org/wiki/Ring_galaxy).

`cell.rs` represents a unit of space within a galaxy, and holds all of the identifying information about that unit of space. The galactic terrain is composed of (a variable language quantity of) cells, and come in two types: [generic gas clouds](https://en.wikipedia.org/wiki/Nebula) and [star systems](https://en.wikipedia.org/wiki/Star_system).


## js

`src/js/*` handles rendering the data created by the rust backend

The `wasm` + `js` apis are loaded into the main application [typescript](https://www.typescriptlang.org/) via async imports
