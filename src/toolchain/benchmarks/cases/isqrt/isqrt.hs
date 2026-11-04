-- Sum of integer square roots over 1..N — Newton's method, integer division heavy.
module Main (main) where

isqrtIter :: Int -> Int -> Int
isqrtIter n x =
  let y = (x + n `div` x) `div` 2
  in if y < x then isqrtIter n y else x

isqrt :: Int -> Int
isqrt n = if n < 2 then n else isqrtIter n n

step :: Int -> Int -> Int
step acc i = acc + isqrt i

main :: IO ()
main = print (foldl step 0 [1 .. 1000000])
