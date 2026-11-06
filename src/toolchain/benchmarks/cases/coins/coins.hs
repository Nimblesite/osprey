coin :: Int -> Int
coin k = case k of
    1 -> 1
    2 -> 5
    3 -> 10
    4 -> 25
    _ -> 50

ways :: Int -> Int -> Int
ways amount kind
    | amount == 0 = 1
    | amount < 0  = 0
    | kind == 0   = 0
    | otherwise   = ways (amount - coin kind) kind + ways amount (kind - 1)

main :: IO ()
main = print (ways 600 5)
