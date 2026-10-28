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
        WinRT.ComWrappersSupport.InitializeComWrappers();
        Application.Start(started =>
        {
            SynchronizationContext.SetSynchronizationContext(new DispatcherQueueSynchronizationContext(DispatcherQueue.GetForCurrentThread()));
            _ = new HostApplication(launch);
        });
        return Environment.ExitCode;
    }
}
