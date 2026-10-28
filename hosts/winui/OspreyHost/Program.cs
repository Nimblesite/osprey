using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;

namespace Osprey.Host.WinUI;

/// <summary>
/// <c>OspreyHost.exe &lt;app.dll&gt; [arguments]</c>: opens one window and draws the Osprey Native
/// application compiled into <c>app.dll</c>. The arguments reach the application's launch context.
/// With <c>--self-test</c> the window is driven by a script and the process exits with its verdict.
/// </summary>
static class Program
{
    [STAThread]
    static int Main(string[] args)
    {
        if (Launch.Parse(args) is not { } launch)
        {
            Console.Error.WriteLine(Launch.Usage);
            return 2;
        }
        if (launch.Shots is { } shots)
        {
            Progress.Begin(shots);
        }
        if (Desktop.Missing() is { } missing)
        {
            return Refused(launch, missing);
        }
        Progress.Note("desktop visible");
        WinRT.ComWrappersSupport.InitializeComWrappers();
        Application.Start(started =>
        {
            Progress.Note("WinUI started");
            SynchronizationContext.SetSynchronizationContext(new DispatcherQueueSynchronizationContext(DispatcherQueue.GetForCurrentThread()));
            _ = new HostApplication(launch);
        });
        Progress.Note($"WinUI ended, exit code {Environment.ExitCode}");
        return Environment.ExitCode;
    }

    // With no desktop to draw on, the reason goes where the person or the self-test will read it.
    static int Refused(Launch launch, string reason)
    {
        Console.Error.WriteLine(reason);
        if (launch.Shots is { } shots)
        {
            Directory.CreateDirectory(shots);
            File.WriteAllText(Path.Combine(shots, "self-test.txt"), $"FAIL {reason}{Environment.NewLine}");
        }
        return 3;
    }
}
