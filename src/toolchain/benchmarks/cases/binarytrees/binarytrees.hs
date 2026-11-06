-- Allocate + traverse many short-lived binary trees — allocation/memory stress.
-- Faithful port of binarytrees.osp: build 1200 trees of depth 13, sum checks.

data Tree = Leaf | Node Tree Tree

make :: Int -> Tree
make 0 = Node Leaf Leaf
make d = Node (make (d - 1)) (make (d - 1))

check :: Tree -> Int
check Leaf = 0
check (Node l r) = 1 + check l + check r

main :: IO ()
main = print (foldl (\acc _ -> acc + check (make 13)) 0 [0 .. 1199 :: Int])
