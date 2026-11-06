-- Count coprime pairs (i,j), 1<=i,j<=N — nested iteration + Euclidean gcd.
gcd' :: Int -> Int -> Int
gcd' a b = if b == 0 then a else gcd' b (a `mod` b)

loopJ :: Int -> Int -> Int -> Int
loopJ _ 0 acc = acc
loopJ i j acc = loopJ i (j - 1) (if gcd' i j == 1 then acc + 1 else acc)

loopI :: Int -> Int -> Int -> Int
loopI 0 _ acc = acc
loopI i n acc = loopI (i - 1) n (loopJ i n acc)

main :: IO ()
main = print (loopI 2000 2000 0)
