using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Windows.Graphics;

namespace Osprey.Host.WinUI;

/// <summary>
/// The one window [WINUI-HOST-THIN]: Mica behind the application's root region, and an info bar
/// that shows every diagnostic the framework or this host reports, so nothing fails silently.
/// </summary>
sealed class HostWindow : Window
{
    const int Width = 1180;
    const int Height = 780;

    readonly InfoBar problems = new() { Severity = InfoBarSeverity.Error, IsOpen = false, Title = "Osprey" };
    readonly Session? session;

    public HostWindow(string library, string[] arguments)
    {
        Title = Path.GetFileNameWithoutExtension(library);
        SystemBackdrop = new MicaBackdrop();
        AppWindow.Resize(new SizeInt32(Width, Height));
        var root = new Border();
        Content = Layout(root);
        session = Started(library, arguments, root);
    }

    Grid Layout(Border root)
    {
        var grid = new Grid();
        grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        grid.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        Grid.SetRow(root, 1);
        grid.Children.Add(problems);
        grid.Children.Add(root);
        return grid;
    }

    Session? Started(string library, string[] arguments, Border root)
    {
        try
        {
            var opened = new Session(Library.Open(library), root, DispatcherQueue, Report, Pick);
            opened.Begin(arguments);
            return opened;
        }
        catch (Exception problem) when (problem is HostException or DllNotFoundException or BadImageFormatException)
        {
            Report([problem.Message]);
            return null;
        }
    }

    /// <summary>Shows diagnostics in the info bar; an envelope without any leaves the last ones showing.</summary>
    void Report(IReadOnlyList<string> diagnostics)
    {
        if (diagnostics.Count == 0)
        {
            return;
        }
        problems.Message = string.Join(Environment.NewLine, diagnostics);
        problems.IsOpen = true;
        System.Diagnostics.Debug.WriteLine(problems.Message);
    }

    async Task<string?> Pick()
    {
        var picker = new Microsoft.Windows.Storage.Pickers.FolderPicker(AppWindow.Id);
        var picked = await picker.PickSingleFolderAsync();
        return picked?.Path;
    }
}
