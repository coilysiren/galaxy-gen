import { Component } from "@angular/core";

@Component({
  selector: "app-root",
  template: "{{ title }}"
})
export class AppComponent {
  public title: string = "galaxy gen app";
}
