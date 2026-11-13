using System.Diagnostics;

namespace Osprey.Host;

/// <summary>
/// The <c>run</c> capability [WINUI-HOST-CAPABILITIES]: one executable started without a shell,
/// its stdout written to the request's slice. The deadline and the output limit are the
/// request's; past either the child is killed and the request fails.
/// </summary>
public static class Processes
{
    /// <summary>Runs the request's executable off the UI thread.</summary>
    public static Task<Completion> Run(string ticket, Ask ask) => Task.Run(async () =>
    {
        var executable = ask.Text(1);
        if (!Path.IsPathFullyQualified(executable))
        {
            return Completion.Failed(ticket, "Executable must be an absolute path");
        }
        try
        {
            return await Launched(ticket, executable, ask);
        }
        catch (Exception problem) when (problem is System.ComponentModel.Win32Exception or IOException or InvalidOperationException or AggregateException)
        {
            return Completion.Failed(ticket, problem.Message);
        }
    });

    static async Task<Completion> Launched(string ticket, string executable, Ask ask)
    {
        var start = new ProcessStartInfo(executable) { RedirectStandardOutput = true, RedirectStandardError = true, UseShellExecute = false };
        foreach (var argument in ask.Texts(2))
        {
            start.ArgumentList.Add(argument);
        }
        var child = Process.Start(start) ?? throw new InvalidOperationException($"{executable} did not start");
        using var deadline = new CancellationTokenSource(TimeSpan.FromMilliseconds(Math.Max(1, ask.Number(3))));
        var stdout = child.StandardOutput.ReadToEndAsync(deadline.Token);
        var stderr = child.StandardError.ReadToEndAsync(deadline.Token);
        try
        {
            await child.WaitForExitAsync(deadline.Token);
            var finished = Finished(ticket, ask, child.ExitCode, await stdout.WaitAsync(deadline.Token), await stderr.WaitAsync(deadline.Token));
            child.Dispose();
            return finished;
        }
        catch (OperationCanceledException)
        {
            child.Kill(entireProcessTree: true);
            // Closing a pipe a read is blocked on waits for that read, so the deadline's answer does not.
            _ = Task.Run(child.Dispose);
            return Completion.Failed(ticket, $"{executable} ran past its deadline");
        }
    }

    static Completion Finished(string ticket, Ask ask, int status, string stdout, string stderr) =>
        stdout.Length > ask.Number(4)
            ? Completion.Failed(ticket, "The output passed its limit")
            : Completion.Done(ticket, Docs.Object(writer =>
            {
                writer.WriteNumber("status", status);
                writer.WriteString("stderr", stderr);
            }), new SliceWrite(ask.Text(6), stdout));
}
