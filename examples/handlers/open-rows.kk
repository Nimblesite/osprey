effect fun write(message : string) : ()

fun forward(callback : () -> e ()) : e ()
  callback()

fun work()
  write("job")

fun logging(label)
  handler
    fun write(message) println(label ++ ":" ++ message)

pub fun main()
  val live = logging("live")
  val test = logging("test")
  live(fn() forward(work))
  test(fn() forward(work))
