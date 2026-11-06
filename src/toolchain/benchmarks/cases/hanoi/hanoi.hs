hanoi :: Int -> Int -> Int
hanoi 0 acc = acc
hanoi n acc = hanoi (n - 1) (hanoi (n - 1) acc + 1)

main :: IO ()
main = print (hanoi 25 0)
