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
        Resources.MergedDictionaries.Add(new XamlControlsResources());
        window = new HostWindow(launch.Library, launch.Arguments);
        window.Activate();
        if (launch is { Script: { } script, Shots: { } shots })
        {
            _ = Tested(new SelfTest(window, script, shots));
        }
    }

    // [WINUI-HOST-SELF-TEST] The process exits with the script's verdict once it has run.
    async Task Tested(SelfTest test)
    {
        Environment.ExitCode = await test.Run();
        Exit();
    }

    public IXamlType GetXamlType(Type type) => controls.GetXamlType(type);

    public IXamlType GetXamlType(string fullName) => controls.GetXamlType(fullName);

    public XmlnsDefinition[] GetXmlnsDefinitions() => controls.GetXmlnsDefinitions();
}
