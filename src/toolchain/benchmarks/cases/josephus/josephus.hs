-- Josephus problem — survivor index for n people, step k=7, via the modular recurrence.
module Main (main) where

step :: Int -> Int -> Int
step acc i = (acc + 7) `mod` i

main :: IO ()
main = print (foldl step 0 [2 .. 10000000])
