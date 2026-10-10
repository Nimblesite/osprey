{-# LANGUAGE BangPatterns #-}

module Main (main) where

import Control.Monad (replicateM)
import Data.Char (isSpace)
import Data.List (foldl')
import System.IO
  ( IOMode (ReadMode)
  , hGetChar
  , isEOF
  , withBinaryFile
  )
import Text.Read (readMaybe)

minstd :: Int
minstd = 16807

modulus :: Int
modulus = 2147483647

bigMod :: Int
bigMod = 1000000007

r1 :: Int -> Int
r1 x = (x * minstd) `mod` modulus

hashAt :: Int -> Int -> Int
hashAt s i = r1 (r1 (s + i))

readFirstLine :: IO String
readFirstLine = do
  eof <- isEOF
  if eof then pure "" else getLine

readSeed :: IO Int
readSeed = do
  line <- readFirstLine
  let m = maybe 0 id (readMaybe (filter (not . isSpace) line))
  if m == 0
    then pure 1
    else do
      cs <- withBinaryFile "/dev/urandom" ReadMode (replicateM 8 . hGetChar)
      let val = foldl (\acc c -> acc * 256 + fromEnum c) 0 cs
      pure (abs val `mod` 2147483646 + 1)

-- Naive top-down merge sort: deal the list into two halves by alternating
-- elements, sort both, merge.
evens :: [Int] -> [Int]
evens [] = []
evens (x : rest) = x : odds rest

odds :: [Int] -> [Int]
odds [] = []
odds (_ : rest) = evens rest

merge :: [Int] -> [Int] -> [Int]
merge [] right = right
merge left [] = left
merge left@(l : ls) right@(r : rs)
  | r < l = r : merge left rs
  | otherwise = l : merge ls right

mergeSort :: [Int] -> [Int]
mergeSort [] = []
mergeSort [only] = [only]
mergeSort xs = merge (mergeSort (evens xs)) (mergeSort (odds xs))

weigh :: [Int] -> Int
weigh xs = foldl' (\ !a (rank, x) -> (a + rank * x) `mod` bigMod) 0 (zip [1 ..] xs)

step :: Int -> Int -> Int
step seed t =
  let s = seed + t * 131
   in weigh (mergeSort [hashAt s i `mod` 100000 | i <- [0 .. 1999]])

main :: IO ()
main = do
  seed <- readSeed
  let acc =
        foldl'
          (\ !a t -> (a + step seed t) `mod` bigMod)
          0
          [0 .. 7]
  print acc
