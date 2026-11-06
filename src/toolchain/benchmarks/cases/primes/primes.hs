-- Count primes below a limit by trial division — integer % in a tight loop.
module Main (main) where

hasFactor :: Int -> Int -> Bool
hasFactor n d
  | d * d > n = False
  | n `mod` d == 0 = True
  | otherwise = hasFactor n (d + 1)

isPrime :: Int -> Bool
isPrime n
  | n < 2 = False
  | otherwise = not (hasFactor n 2)

tally :: Int -> Int -> Int
tally acc n = if isPrime n then acc + 1 else acc

main :: IO ()
main = print (foldl tally 0 [2 .. 199999 :: Int])
