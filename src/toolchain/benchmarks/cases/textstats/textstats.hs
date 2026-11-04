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

r1 :: Int -> Int
r1 x = (x * minstd) `mod` modulus

hashAt :: Int -> Int -> Int
hashAt s i = r1 (r1 (s + i))

vocab :: [String]
vocab = ["the", "quick", "brown", "fox", "jumps", "over", "lazy", "dog"]

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

score :: String -> Int
score w =
  length w
    + (if 'o' `elem` w then 7 else 0)
    + (if take 1 w == "t" then 3 else 0)

main :: IO ()
main = do
  seed <- readSeed
  let acc =
        foldl'
          (\ !a i -> a + score (vocab !! (hashAt seed i `mod` 8)))
          0
          [0 .. 199999]
  print acc
