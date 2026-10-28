using Microsoft.UI;
using Microsoft.UI.Windowing;
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
    readonly List<string> reported = [];
    readonly Session? session;

    public HostWindow(string library, string[] arguments)
    {
        Progress.Note("window created");
        Title = Path.GetFileNameWithoutExtension(library);
        SystemBackdrop = new MicaBackdrop();
        Progress.Note("backdrop set");
        Fit();
        var root = new Border();
        Content = Layout(root);
        Progress.Note("content set");
        session = Started(library, arguments, root);
        Progress.Note(session is null ? "application not started" : "application started");
    }

    /// <summary>Every diagnostic shown since the window opened.</summary>
    public IReadOnlyList<string> Problems => reported;

    /// <summary>The window's handle, for capturing what it shows.</summary>
    public nint Handle => Win32Interop.GetWindowFromWindowId(AppWindow.Id);

    /// <summary>Chooses <paramref name="row"/> as a click on it would; false when its list takes no choice.</summary>
    public bool Choose(ListViewItem row) => session?.Choose(row) ?? false;

    /// <summary>Presses and releases a key in the scene on screen; false when no scene is drawn.</summary>
    public bool Press(string key) => session?.Press(key) ?? false;

    // The window opens at its own size, or smaller on a screen that cannot hold it, centred.
    void Fit()
    {
        var area = DisplayArea.GetFromWindowId(AppWindow.Id, DisplayAreaFallback.Primary).WorkArea;
        var size = new SizeInt32(Math.Min(Width, area.Width), Math.Min(Height, area.Height));
        AppWindow.MoveAndResize(new RectInt32(area.X + (area.Width - size.Width) / 2, area.Y + (area.Height - size.Height) / 2, size.Width, size.Height));
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
            var opened = new Session(Library.Open(library), Store.RootFor(library), root, DispatcherQueue, Report, Pick);
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
        reported.AddRange(diagnostics);
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
