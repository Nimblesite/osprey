using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Markup;
using Microsoft.UI.Xaml.XamlTypeInfo;

namespace Osprey.Host.WinUI;

/// <summary>
/// The application object, built in code [WINUI-HOST-NO-XAML]: it supplies the WinUI controls'
/// type metadata itself, which a XAML application would have generated, and loads their styles.
/// </summary>
sealed partial class HostApplication(string library, string[] arguments) : Application, IXamlMetadataProvider
{
    readonly XamlControlsXamlMetaDataProvider controls = new();
    HostWindow? window;

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        Resources.MergedDictionaries.Add(new XamlControlsResources());
        window = new HostWindow(library, arguments);
        window.Activate();
    }

    public IXamlType GetXamlType(Type type) => controls.GetXamlType(type);

    public IXamlType GetXamlType(string fullName) => controls.GetXamlType(fullName);

    public XmlnsDefinition[] GetXmlnsDefinitions() => controls.GetXmlnsDefinitions();
}
