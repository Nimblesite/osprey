-- Sum of modular exponentiation: sum of i^20 mod P for i in 1..N, naive repeated multiply.
import Data.List (foldl')

p :: Int
p = 1000000007

powmod :: Int -> Int -> Int -> Int
powmod _ 0 acc = acc
powmod base e acc = powmod base (e - 1) ((acc * base) `mod` p)

step :: Int -> Int -> Int
step acc i = (acc + powmod i 20 1) `mod` p

main :: IO ()
main = print (foldl' step 0 [1 .. 999999])
