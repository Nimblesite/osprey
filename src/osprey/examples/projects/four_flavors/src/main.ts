// osprey: namespace=shop
import { price } from "./pricing";

/** A zero divisor answers zero, so a price never fails. */
const zeroed = handler<Arith>({ remainderByZero: (_lhs) => 0 });

console.log(`price=${zeroed(() => price(7))}`);
