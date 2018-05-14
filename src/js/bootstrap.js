import { platformBrowserDynamic } from "@angular/platform-browser-dynamic";

import { setupMainScript } from "./setup";
import { AppModule } from "./app/app.module";
import "./assets/polyfills";

setupMainScript();

platformBrowserDynamic()
  .bootstrapModule(AppModule)
  .catch(err => console.log(err));
