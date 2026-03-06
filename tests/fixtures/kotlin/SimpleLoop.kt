fun sumRange(n: Int): Int {
    var total = 0
    for (i in 0 until n) {
        total += i
    }
    return total
}

fun main() {
    val result = sumRange(10)
    println("Sum: $result")
}
