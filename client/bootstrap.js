import { MainScript } from "./main";

import("./assets/built-wasm/galaxy_gen").then((rust) => new MainScript(rust));
