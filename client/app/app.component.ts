import { Component, OnInit } from "@angular/core";
import "assets/curled/fontawesome.js";

@Component({
  selector: "app-root",
  template: `
    <header-component></header-component>
    <router-outlet></router-outlet>
  `,
  styles: [
    "./base.scss",
  ]
})
export class AppComponent implements OnInit {

  public ngOnInit(): void {
    import("assets/built-wasm/galaxy_gen").then((js: any) => {
      js.greet("Rust and WebAssembly?");
    });
  }
}
