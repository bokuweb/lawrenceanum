
  import { createRoot } from "react-dom/client";
  import App from "./app/App.tsx";
  import "./styles/index.css";
  import { registerWebMcp } from "./app/data/webmcp";

  const disposeWebMcp = registerWebMcp();
  if (import.meta.hot) import.meta.hot.dispose(disposeWebMcp);

  createRoot(document.getElementById("root")!).render(<App />);
