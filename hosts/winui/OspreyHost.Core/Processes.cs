using System.Diagnostics;
using System.Text.Json;

namespace Osprey.Host;

/// <summary>
/// The <c>run</c> capability [WINUI-HOST-CAPABILITIES]: one executable started without a shell,
/// its stdout written to the request's slice. The deadline and the output limit are the
/// request's; past either the child is killed and the request fails.
/// </summary>
public static class Processes
{
    /// <summary>Runs the request's executable off the UI thread.</summary>
    public static Task<Completion> Run(string ticket, JsonElement request) => Task.Run(async () =>
    {
        var executable = Envelopes.Text(request, "executable");
        if (!Path.IsPathFullyQualified(executable))
        {
            return Completion.Failed(ticket, "Executable must be an absolute path");
        }
        try
        {
            return await Launched(ticket, executable, request);
        }
        catch (Exception problem) when (problem is System.ComponentModel.Win32Exception or IOException or InvalidOperationException or AggregateException)
        {
            return Completion.Failed(ticket, problem.Message);
        }
    });

    static async Task<Completion> Launched(string ticket, string executable, JsonElement request)
    {
        var start = new ProcessStartInfo(executable) { RedirectStandardOutput = true, RedirectStandardError = true, UseShellExecute = false };
        foreach (var argument in Envelopes.Items(request, "arguments", item => item.GetString() ?? ""))
        {
            start.ArgumentList.Add(argument);
        }
        using var child = Process.Start(start) ?? throw new InvalidOperationException($"{executable} did not start");
        using var deadline = new CancellationTokenSource(TimeSpan.FromMilliseconds(Math.Max(1, Envelopes.Number(request, "timeout"))));
        var stdout = child.StandardOutput.ReadToEndAsync(deadline.Token);
        var stderr = child.StandardError.ReadToEndAsync(deadline.Token);
        try
        {
            await child.WaitForExitAsync(deadline.Token);
            return Finished(ticket, request, child.ExitCode, await stdout, await stderr);
        }
        catch (OperationCanceledException)
        {
            child.Kill(entireProcessTree: true);
            return Completion.Failed(ticket, $"{executable} ran past its deadline");
        }
    }

    static Completion Finished(string ticket, JsonElement request, int status, string stdout, string stderr) =>
        stdout.Length > Envelopes.Number(request, "limit")
            ? Completion.Failed(ticket, "The output passed its limit")
            : Completion.Done(ticket, Json.Object(writer =>
            {
                writer.WriteNumber("status", status);
                writer.WriteString("stderr", stderr);
            }), new SliceWrite(Envelopes.Text(request, "slice"), stdout));
}
