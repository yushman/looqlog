// Bundle entry point. The served HTML loads only this module (app-shell spec: "No
// inline application script SHALL remain in the served HTML") — everything else is
// Web Components registering themselves on import.

import "./components/looq-app";
import "./style.css";

import { initTheme } from "./theme";

initTheme();
