effect fun ask() : int
effect fun name() : string

val answer = handler
  fun ask() 41

fun work()
  ask() + 1

fun reading(person)
  handler
    fun name() person

fun greet()
  "Hello, " ++ name() ++ "!"

pub fun main()
  val ada = reading("Ada")
  val grace = reading("Grace")
  val first = answer(work)
  val second = answer(work)
  val hello-ada = ada(greet)
  val hello-grace = grace(greet)
  val hello-again = ada(greet)
  println(first.show ++ "\n" ++ second.show ++ "\n" ++ hello-ada ++ "\n" ++ hello-grace ++ "\n" ++ hello-again)
