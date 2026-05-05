import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Overlay from "./Overlay";
import "./app.css";

// Tauri sets the window label in the URL — use it to decide which UI to render
const isOverlay = window.location.pathname.includes("overlay") ||
  new URLSearchParams(window.location.search).get("window") === "overlay";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {isOverlay ? <Overlay /> : <App />}
  </React.StrictMode>
);