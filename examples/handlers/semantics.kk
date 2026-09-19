effect ctl ask() : int

fun work()
  ask() + 1

val replace = handler
  ctl ask() resume(41)
val around = handler
  ctl ask() resume(41) + 100
val no-resume = handler
  ctl ask() 0
val dead-resume = handler
  ctl ask() if False then resume(0) else 0

pub fun main()
  val replacement = replace(work)
  val resumed = around(work)
  val returned = no-resume(work)
  val dead-branch = dead-resume(work)
  println("replace=" ++ replacement.show ++ "\nresume=" ++ resumed.show ++ "\nno-resume=" ++ returned.show ++ "\ndead-resume=" ++ dead-branch.show)
