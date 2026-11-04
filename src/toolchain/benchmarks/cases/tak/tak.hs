sub :: Int -> Int -> Int
sub a b = a - b

tak :: Int -> Int -> Int -> Int
tak x y z =
  if x > y
    then tak (tak (sub x 1) y z) (tak (sub y 1) z x) (tak (sub z 1) x y)
    else z

main :: IO ()
main = print (tak 32 16 8)
