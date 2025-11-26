import App from "./App.tsx";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource/fira-sans";

// TODO:
// import "./index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
