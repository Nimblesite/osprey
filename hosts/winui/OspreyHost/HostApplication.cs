using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Markup;
using Microsoft.UI.Xaml.XamlTypeInfo;

namespace Osprey.Host.WinUI;

/// <summary>
/// The application object, built in code [WINUI-HOST-NO-XAML]: it supplies the WinUI controls'
/// type metadata itself, which a XAML application would have generated, and loads their styles.
/// </summary>
sealed partial class HostApplication(Launch launch) : Application, IXamlMetadataProvider
{
    readonly XamlControlsXamlMetaDataProvider controls = new();
    HostWindow? window;

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        UnhandledException += (_, failed) => Progress.Note($"unhandled {failed.Exception.GetType().Name} 0x{failed.Exception.HResult:X8}: {failed.Message}");
        Progress.Note("launched");
        Resources.MergedDictionaries.Add(new XamlControlsResources());
        Progress.Note("control styles loaded");
        window = new HostWindow(launch.Library, launch.Arguments);
        window.Activate();
        Progress.Note("window activated");
        if (launch is { Script: { } script, Shots: { } shots })
        {
            _ = Tested(new SelfTest(window, script, shots));
        }
    }

    // [WINUI-HOST-SELF-TEST] The process exits with the script's verdict once it has run.
    // Anything the script did not expect is written to the trail and fails the test, rather than leaving the window open.
    async Task Tested(SelfTest test)
    {
        try
        {
            Environment.ExitCode = await test.Run();
        }
        catch (Exception problem)
        {
            Progress.Note($"self-test stopped: {problem}");
            Environment.ExitCode = 1;
        }
        Progress.Note($"self-test finished, exit code {Environment.ExitCode}");
        Exit();
    }

    public IXamlType GetXamlType(Type type) => controls.GetXamlType(type);

    public IXamlType GetXamlType(string fullName) => controls.GetXamlType(fullName);

    public XmlnsDefinition[] GetXmlnsDefinitions() => controls.GetXmlnsDefinitions();
}
