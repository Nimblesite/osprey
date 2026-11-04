namespace Osprey.Host.WinUI;

/// <summary>
/// A self-test's trail [WINUI-HOST-SELF-TEST]: each phase of start-up and each step, appended and
/// flushed as it happens, so a process that dies leaves the last thing it reached behind.
/// </summary>
static class Progress
{
    static string? file;

    /// <summary>Starts the trail in <paramref name="shots"/>; without it, notes go nowhere.</summary>
    public static void Begin(string shots)
    {
        Directory.CreateDirectory(shots);
        file = Path.Combine(shots, "progress.txt");
        File.WriteAllText(file, "");
        Note("started");
    }

    /// <summary>Appends one line, stamped with the time it was reached.</summary>
    public static void Note(string reached)
    {
        if (file is not null)
        {
            File.AppendAllText(file, $"{DateTime.Now:HH:mm:ss.fff} {reached}{Environment.NewLine}");
        }
    }
}
