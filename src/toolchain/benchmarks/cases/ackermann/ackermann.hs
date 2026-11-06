-- Ackermann–Péter function — deep, non-tail mutual self-recursion.
add' :: Int -> Int -> Int
add' a b = a + b

sub' :: Int -> Int -> Int
sub' a b = a - b

ack :: Int -> Int -> Int
ack m n
  | m == 0 = add' n 1
  | n == 0 = ack (sub' m 1) 1
  | otherwise = ack (sub' m 1) (ack m (sub' n 1))

main :: IO ()
main = print (ack 3 10)
