-- Collatz (3n+1) stopping time summed over 1..N — integer division (n/2) in deep recursion.
module Main (main) where

collatz :: Int -> Int
collatz 1 = 0
collatz n
  | even n    = 1 + collatz (n `div` 2)
  | otherwise = 1 + collatz (3 * n + 1)

step :: Int -> Int -> Int
step acc i = acc + collatz i

main :: IO ()
main = print (foldl step 0 [1 .. 100000])
