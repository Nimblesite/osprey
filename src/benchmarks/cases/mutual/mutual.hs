isEven :: Int -> Bool
isEven 0 = True
isEven n = isOdd (n - 1)

isOdd :: Int -> Bool
isOdd 0 = False
isOdd n = isEven (n - 1)

step :: Int -> Int -> Int
step acc i = if isEven (i `mod` 1000) then acc + 1 else acc

main :: IO ()
main = print (foldl step 0 [1 .. 129999])
