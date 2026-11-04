-- Sum of decimal digit-sums over 1..N — integer division (n/10) and modulo in recursion.
module Main (main) where

digsum :: Int -> Int
digsum n
  | n < 10    = n
  | otherwise = n `mod` 10 + digsum (n `div` 10)

step :: Int -> Int -> Int
step acc i = acc + digsum i

main :: IO ()
main = print (foldl step 0 [1 .. 2000000])
