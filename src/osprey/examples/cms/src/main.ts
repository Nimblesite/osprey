// osprey: namespace=cms
/** The Git CMS server: read the configuration, then serve the editor and its API. [CMS] */
import { serve } from "./app.ts";
import { loadConfig } from "./settings.ts";

const config = loadConfig();
switch (config.kind) {
  case "success": {
    const _ = serve(config.value, () => {
      console.log(`Osprey CMS editing ${config.value.repo} on http://127.0.0.1:${config.value.port}/`);
    });
    break;
  }
  case "error":
    console.log(`Osprey CMS cannot start: ${config.message}`);
    break;
}
