class SimpleLoop
{
    static int SumRange(int n)
    {
        int total = 0;
        for (int i = 0; i < n; i++)
        {
            total += i;
        }
        return total;
    }

    static void Main()
    {
        int result = SumRange(10);
        System.Console.WriteLine($"Sum: {result}");
    }
}
