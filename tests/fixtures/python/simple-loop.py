"""Simple loop for basic stepping and variable inspection."""


def sum_range(n: int) -> int:
    total = 0
    for i in range(n):
        total += i
    return total


if __name__ == "__main__":
    result = sum_range(10)
    print(f"Sum: {result}")
