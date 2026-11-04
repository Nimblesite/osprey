-- Triple-nested counting loop accumulating (i*j*k) mod P — nested iteration.
modP :: Int
modP = 1000000007

loopK :: Int -> Int -> Int -> Int -> Int
loopK i j k acc
  | k == 0 = acc
  | otherwise = loopK i j (k - 1) ((acc + i * j * k) `mod` modP)

loopJ :: Int -> Int -> Int -> Int -> Int
loopJ i j n acc
  | j == 0 = acc
  | otherwise = loopJ i (j - 1) n (loopK i j n acc)

loopI :: Int -> Int -> Int -> Int
loopI i n acc
  | i == 0 = acc
  | otherwise = loopI (i - 1) n (loopJ i n n acc)

main :: IO ()
main = print (loopI 250 250 0)
