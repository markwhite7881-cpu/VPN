import React, { Suspense } from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

// Mobile UI is a separate bundle chunk — desktop users never pay for it.
const MobileApp = React.lazy(() => import("./mobile/MobileApp"));

// Android (Tauri WebView) or an explicit ?mobile=1 override for testing
// the mobile shell in a desktop browser.
const isMobile =
  /Android/i.test(navigator.userAgent) ||
  new URLSearchParams(window.location.search).has("mobile");

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {isMobile ? (
      <Suspense fallback={null}>
        <MobileApp />
      </Suspense>
    ) : (
      <App />
    )}
  </React.StrictMode>
);
