def sum_range(n)
  total = 0
  (0...n).each do |i|
    total += i
  end
  total
end

result = sum_range(10)
puts "Sum: #{result}"
