// osprey: namespace=shop
// osprey: module=Pricing
/** Prices, built from a module in each of the other three flavors. */
import { cents } from "./units";
import { nearest } from "./rounding";
import { with_tax } from "./tax";

/** A price in cents, taxed and rounded to the nearest five cents. */
export function price(dollars) {
  return nearest(5, with_tax(cents(dollars)));
}
