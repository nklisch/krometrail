func sumRange(_ n: Int) -> Int {
    var total = 0
    for i in 0..<n {
        total += i
    }
    return total
}

let result = sumRange(10)
print("Sum: \(result)")
