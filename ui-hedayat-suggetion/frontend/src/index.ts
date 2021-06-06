import "./view/header/page-header";
import "./view/header/main-header";
import "./view/header/right-header";
import "./view/navigation/sidebar-navi";
import "./router";

import { Store } from "./AppState";

// Initialize Values.
Store.getInstance().setState({
  loggedin: false,
  agentKey: "",
  isAdmin: false,
});
