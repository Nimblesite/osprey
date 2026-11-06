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

step :: Int -> Int -> Int
step seed t =
  let s = seed + t * 131
      xs = [hashAt s i `mod` 1000 | i <- [0 .. 3999]]
      sum' = sum xs
      below = length (filter (< 500) xs)
   in sum' + below

main :: IO ()
main = do
  seed <- readSeed
  let acc =
        foldl'
          (\ !a t -> (a + step seed t) `mod` bigMod)
          0
          [0 .. 7]
  print acc
