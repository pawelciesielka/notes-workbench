import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";
import { applyStoredTheme } from "./lib/theme";
import { api } from "./lib/api";

api.startMark(`skrypt uruchomiony (${Math.round(performance.now())} ms od otwarcia strony)`);

applyStoredTheme();

// Plik upuszczony poza Notatką nie może otworzyć się w oknie zamiast aplikacji.
for (const type of ["dragover", "drop"] as const) {
  window.addEventListener(type, (e) => {
    if (e.dataTransfer?.types.includes("Files")) e.preventDefault();
  });
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
