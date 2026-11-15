// osprey: namespace=routing
import { serve } from "./api.ts";

const _ = serve(3000, () => {
  console.log("JSON API listening on port 3000");
});
