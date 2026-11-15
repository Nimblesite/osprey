// osprey: namespace=routing
import { serve } from "./service.ts";

const _ = serve(3000, () => {
  console.log("Express started on port 3000");
});
