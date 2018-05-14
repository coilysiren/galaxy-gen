// angular polyfills
import "core-js/es7/reflect";
import "zone.js/dist/zone";

import { platformBrowserDynamic } from "@angular/platform-browser-dynamic";

import { setupMainScript } from "./setup";
import { AppModule } from "./app/app.module";

setupMainScript();

platformBrowserDynamic()
  .bootstrapModule(AppModule)
  .catch(err => console.log(err));
