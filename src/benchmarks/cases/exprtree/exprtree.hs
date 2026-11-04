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

data Expr = Lit !Int | Add Expr Expr | Mul Expr Expr

build :: Int -> Int -> Int -> Expr
build s idx depth
  | depth == 0 = Lit (hashAt s idx `mod` 1000)
  | otherwise =
      let op = hashAt s idx `mod` 2
          l = build s (idx * 2 + 1) (depth - 1)
          r = build s (idx * 2 + 2) (depth - 1)
       in if op == 0 then Add l r else Mul l r

eval :: Expr -> Int
eval (Lit v) = v
eval (Add l r) = (eval l + eval r) `mod` bigMod
eval (Mul l r) = (eval l * eval r) `mod` bigMod

main :: IO ()
main = do
  seed <- readSeed
  let acc =
        foldl'
          (\ !a t -> (a + eval (build (seed + t * 7) 1 14)) `mod` bigMod)
          0
          [0 .. 9]
  print acc
