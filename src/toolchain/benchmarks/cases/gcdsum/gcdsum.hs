-- Sum of gcd(i, K) for i in 1..N — Euclidean-algorithm recursion (modulo heavy).
module Main (main) where

myGcd :: Int -> Int -> Int
myGcd a 0 = a
myGcd a b = myGcd b (a `mod` b)

step :: Int -> Int -> Int
step acc i = acc + myGcd i 1234567

main :: IO ()
main = print (foldl step 0 [1 .. 1999999])
