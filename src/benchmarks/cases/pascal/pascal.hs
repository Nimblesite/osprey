-- Binomial coefficient via naive (un-memoised) Pascal recursion C(n,k)=C(n-1,k-1)+C(n-1,k).
binom :: Int -> Int -> Int
binom n k
  | k == 0    = 1
  | k == n    = 1
  | otherwise = binom (n - 1) (k - 1) + binom (n - 1) k

main :: IO ()
main = print (binom 27 13)
