-- Naive recursive Fibonacci — exercises raw function-call + recursion overhead.
add :: Int -> Int -> Int
add a b = a + b

sub :: Int -> Int -> Int
sub a b = a - b

fib :: Int -> Int
fib 0 = 0
fib 1 = 1
fib n = add (fib (sub n 1)) (fib (sub n 2))

main :: IO ()
main = print (fib 35)
