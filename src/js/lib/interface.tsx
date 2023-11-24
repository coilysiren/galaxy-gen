import React from "react";
import "./styles.css";

function MyButton() {
  return <button type="button" className="btn btn-primary">"I'm a button"</button>;
}

export default function MyApp() {
  return (
    <div>
      <h1>Welcome to my app</h1>
      <MyButton />
    </div>
  );
}
