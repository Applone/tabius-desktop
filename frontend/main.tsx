import ReactDOM from "react-dom/client";
import { App } from "./App";
import { StorageGate } from "./StorageGate";
import "./style.css";

const root = document.getElementById("root");
if (!root) throw new Error("Missing app root");
ReactDOM.createRoot(root).render(
  <StorageGate>
    <App />
  </StorageGate>,
);
