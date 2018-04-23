import { Component, OnInit } from "@angular/core";
const jsWasmPromise: any = import("./../assets/built-wasm/galaxy_gen");

@Component({
  selector: "app-root",
  template: `
  `,
  styles: [
    "./base.scss",
  ]
})
export class AppComponent implements OnInit {

  public ngOnInit(): void {
    jsWasmPromise.then((js: any) => {
      js.greet("Rust and WebAssembly?");
    });
  }
}
