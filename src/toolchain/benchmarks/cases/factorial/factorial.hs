-- Factorial-style product 1*2*...*N taken mod 1000000007 (matches factorial.osp).
import Data.List (foldl')

modP :: Int
modP = 1000000007

step :: Int -> Int -> Int
step acc i = (acc * i) `mod` modP

main :: IO ()
main = print (foldl' step 1 [1 .. 10000000 :: Int])
