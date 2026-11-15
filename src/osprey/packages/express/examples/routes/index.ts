// osprey: namespace=routing
import { serve } from "./tasks.ts";

const port = 3000;

const _ = serve(port, () => {
  console.log(`Routes example listening on port ${port}`);
});
