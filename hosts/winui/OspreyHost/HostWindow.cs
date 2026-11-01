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
sealed partial class HostWindow : Window
{
    const int Width = 1180;
    const int Height = 780;
    const int OffScreen = -20000;

    readonly InfoBar problems = new() { Severity = InfoBarSeverity.Error, IsOpen = false, Title = "Osprey" };
    readonly List<string> reported = [];
    readonly Session? session;

    public HostWindow(string library, string[] arguments, bool steady)
    {
        Progress.Note("window created");
        Title = Sandbox.Name(library);
        SystemBackdrop = new MicaBackdrop();
        Progress.Note("backdrop set");
        Fit();
        // Under test the content ignores the real pointer [WINUI-HOST-SELF-TEST]: the script acts through
        // automation peers and scene events, and a person's mouse passing over the window would
        // otherwise leave hover states in its pictures.
        var root = new Border { IsHitTestVisible = !steady };
        Content = Layout(root);
        Progress.Note("content set");
        session = Started(library, arguments, root, steady);
        if (session?.CanvasSize() is { } canvas)
        {
            Fit(canvas.Width, canvas.Height);
        }
        Progress.Note(session is null ? "application not started" : "application started");
    }

    /// <summary>Every diagnostic shown since the window opened.</summary>
    public IReadOnlyList<string> Problems => reported;

    /// <summary>Physical pixels per window unit on the window's display: 1.5 at 150%.</summary>
    public double Scale => GetDpiForWindow(Handle) / 96.0;

    /// <summary>The window's handle, for capturing what it shows.</summary>
    public nint Handle => Win32Interop.GetWindowFromWindowId(AppWindow.Id);

    /// <summary>Chooses <paramref name="row"/> as a click on it would; false when its list takes no choice.</summary>
    public bool Choose(ListViewItem row) => session?.Choose(row) ?? false;

    /// <summary>Lets the steady clock send <paramref name="frames"/> more frames; false when nothing animates.</summary>
    public bool Grant(long frames) => session?.Grant(frames) ?? false;

    /// <summary>The frames granted and not yet sent.</summary>
    public long Owed => session?.Owed ?? 0;

    /// <summary>Presses and releases a key in the scene on screen; false when no scene is drawn.</summary>
    public bool Press(string key) => session?.Press(key) ?? false;

    /// <summary>Clicks the scene on screen at a point in its units; false when no scene is drawn.</summary>
    public bool Tap(double x, double y) => session?.Tap(x, y) ?? false;

    /// <summary>
    /// Shows the window at its own size beyond every screen, without activating it and out of the
    /// task switcher [WINUI-HOST-SELF-TEST]. It is still composed, so it draws and captures as a
    /// window on screen does, at the size a screen of any resolution would not change.
    /// </summary>
    public void ShowOffScreen()
    {
        AppWindow.IsShownInSwitchers = false;
        AppWindow.Move(new PointInt32(OffScreen, OffScreen));
        AppWindow.Show(activateWindow: false);
    }

    // The window opens at its own size in window units, scaled to the display, or smaller on a screen
    // that cannot hold it, centred.
    void Fit()
    {
        var area = DisplayArea.GetFromWindowId(AppWindow.Id, DisplayAreaFallback.Primary).WorkArea;
        var size = new SizeInt32(Math.Min((int)(Width * Scale), area.Width), Math.Min((int)(Height * Scale), area.Height));
        AppWindow.MoveAndResize(new RectInt32(area.X + (area.Width - size.Width) / 2, area.Y + (area.Height - size.Height) / 2, size.Width, size.Height));
    }

    // An application that is one sized scene opens with exactly that scene in its client area, in
    // window units scaled to the display, as the GPUI host opens a bare canvas, centred again.
    void Fit(double width, double height)
    {
        AppWindow.ResizeClient(new SizeInt32((int)Math.Ceiling(width * Scale), (int)Math.Ceiling(height * Scale)));
        var area = DisplayArea.GetFromWindowId(AppWindow.Id, DisplayAreaFallback.Primary).WorkArea;
        AppWindow.Move(new PointInt32(area.X + (area.Width - AppWindow.Size.Width) / 2, area.Y + (area.Height - AppWindow.Size.Height) / 2));
    }

    [System.Runtime.InteropServices.LibraryImport("user32.dll")]
    private static partial uint GetDpiForWindow(nint window);

    Grid Layout(Border root)
    {
        var grid = new Grid();
        Renderer.Place(grid, problems, across: false, GridLength.Auto);
        Renderer.Place(grid, root, across: false, Renderer.Rest);
        return grid;
    }

    Session? Started(string library, string[] arguments, Border root, bool steady)
    {
        try
        {
            var opened = new Session(Library.Open(library), Store.RootFor(library), root, DispatcherQueue, Report, new Pickers(Pick, PickFile)) { Steady = steady };
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

    /// <summary>
    /// One file of <paramref name="types"/>, chosen in Windows' own picker, or null when cancelled. A
    /// sandboxed app reads only what it was granted through the picker's broker, so the chosen file
    /// is copied into its LocalState and that copy's path is returned [WINUI-HOST-SANDBOX].
    /// </summary>
    async Task<string?> PickFile(IReadOnlyList<string> types)
    {
        var picker = new Windows.Storage.Pickers.FileOpenPicker();
        WinRT.Interop.InitializeWithWindow.Initialize(picker, Handle);
        foreach (var type in types.DefaultIfEmpty("*"))
        {
            picker.FileTypeFilter.Add(type == "*" ? type : "." + type.TrimStart('.'));
        }
        var picked = await picker.PickSingleFileAsync();
        if (picked is null || !Sandbox.InAppContainer())
        {
            return picked?.Path;
        }
        var chosen = await Windows.Storage.ApplicationData.Current.LocalFolder.CreateFolderAsync("chosen", Windows.Storage.CreationCollisionOption.OpenIfExists);
        var copy = await picked.CopyAsync(chosen, picked.Name, Windows.Storage.NameCollisionOption.ReplaceExisting);
        return copy.Path;
    }

    async Task<string?> Pick()
    {
        var picker = new Microsoft.Windows.Storage.Pickers.FolderPicker(AppWindow.Id);
        var picked = await picker.PickSingleFolderAsync();
        return picked?.Path;
    }
}
