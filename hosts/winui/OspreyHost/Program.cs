using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;

namespace Osprey.Host.WinUI;

/// <summary>
/// <c>OspreyHost.exe &lt;app.dll&gt; [arguments]</c>: opens one window and draws the Osprey Native
/// application compiled into <c>app.dll</c>. The arguments reach the application's launch context.
/// </summary>
static class Program
{
    [STAThread]
    static int Main(string[] args)
    {
        if (args.Length == 0)
        {
            Console.Error.WriteLine("usage: OspreyHost.exe <app.dll> [arguments...]");
            return 2;
        }
        WinRT.ComWrappersSupport.InitializeComWrappers();
        Application.Start(started =>
        {
            SynchronizationContext.SetSynchronizationContext(new DispatcherQueueSynchronizationContext(DispatcherQueue.GetForCurrentThread()));
            _ = new HostApplication(args[0], args[1..]);
        });
        return 0;
    }
}
