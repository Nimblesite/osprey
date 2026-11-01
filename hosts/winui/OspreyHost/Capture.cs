using System.Runtime.InteropServices;
using System.Runtime.InteropServices.WindowsRuntime;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Graphics.Imaging;

namespace Osprey.Host.WinUI;

/// <summary>
/// What the window shows, saved as PNG for the self-test [WINUI-HOST-SELF-TEST]: the whole window as
/// Windows composes it, Mica included, the XAML content alone as WinUI renders it, and the window's
/// client area as the screen shows it.
/// </summary>
static partial class Capture
{
    const uint RenderFullContent = 2;
    const uint SourceCopy = 0x00CC0020;
    const uint CaptureLayered = 0x40000000;
    const string ContentSite = "Microsoft.UI.Content.DesktopChildSiteBridge";
    const uint RgbColours = 0;
    const ushort BitsPerPixel = 32;
    const double Dpi = 96;

    [StructLayout(LayoutKind.Sequential)]
    struct Bounds
    {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [StructLayout(LayoutKind.Sequential)]
    struct BitmapHeader
    {
        public uint Size;
        public int Width;
        public int Height;
        public ushort Planes;
        public ushort BitCount;
        public uint Compression;
        public uint SizeImage;
        public int XPelsPerMeter;
        public int YPelsPerMeter;
        public uint ColoursUsed;
        public uint ColoursImportant;
    }

    /// <summary>The window as Windows composes it, saved to <paramref name="path"/>.</summary>
    public static async Task Window(nint window, string path)
    {
        var (width, height, pixels) = Pixels(window);
        await Save(path, BitmapAlphaMode.Ignore, width, height, pixels);
    }

    /// <summary>The XAML content as WinUI renders it, saved to <paramref name="path"/>.</summary>
    public static async Task Content(UIElement content, string path)
    {
        var target = new RenderTargetBitmap();
        await target.RenderAsync(content);
        var pixels = await target.GetPixelsAsync();
        await Save(path, BitmapAlphaMode.Premultiplied, target.PixelWidth, target.PixelHeight, pixels.ToArray());
    }

    /// <summary>
    /// The XAML content's area copied from the screen, saved to <paramref name="path"/>: what a person
    /// looking at the window sees, after Windows has composed it with everything on the desktop. The
    /// area is the window WinUI hosts the content in, which lines up with the content's own picture.
    /// </summary>
    public static async Task Screen(nint window, string path)
    {
        var site = FindWindowEx(window, 0, ContentSite, null);
        GetWindowRect(site == 0 ? window : site, out var bounds);
        var (width, height) = (bounds.Right - bounds.Left, bounds.Bottom - bounds.Top);
        var pixels = Copied(width, height, context =>
        {
            var screen = GetDC(0);
            try { BitBlt(context, 0, 0, width, height, screen, bounds.Left, bounds.Top, SourceCopy | CaptureLayered); }
            finally { ReleaseDC(0, screen); }
        });
        await Save(path, BitmapAlphaMode.Ignore, width, height, pixels);
    }

    static async Task Save(string path, BitmapAlphaMode alpha, int width, int height, byte[] pixels)
    {
        using var file = File.Create(path);
        var encoder = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, file.AsRandomAccessStream());
        encoder.SetPixelData(BitmapPixelFormat.Bgra8, alpha, (uint)width, (uint)height, Dpi, Dpi, pixels);
        await encoder.FlushAsync();
    }

    static (int Width, int Height, byte[] Pixels) Pixels(nint window)
    {
        GetWindowRect(window, out var bounds);
        var (width, height) = (bounds.Right - bounds.Left, bounds.Bottom - bounds.Top);
        return (width, height, Copied(width, height, context => PrintWindow(window, context, RenderFullContent)));
    }

    // The pixels <paramref name="draw"/> leaves in a width by height bitmap, top row first.
    static byte[] Copied(int width, int height, Action<nint> draw)
    {
        var header = new BitmapHeader { Size = (uint)Marshal.SizeOf<BitmapHeader>(), Width = width, Height = -height, Planes = 1, BitCount = BitsPerPixel };
        var context = CreateCompatibleDC(0);
        var bitmap = CreateDIBSection(context, header, RgbColours, out var bits, 0, 0);
        var previous = SelectObject(context, bitmap);
        try
        {
            draw(context);
            var pixels = new byte[width * height * 4];
            Marshal.Copy(bits, pixels, 0, pixels.Length);
            return pixels;
        }
        finally
        {
            SelectObject(context, previous);
            DeleteObject(bitmap);
            DeleteDC(context);
        }
    }

    [LibraryImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool GetWindowRect(nint window, out Bounds bounds);

    [LibraryImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool PrintWindow(nint window, nint context, uint flags);

    [LibraryImport("user32.dll", EntryPoint = "FindWindowExW", StringMarshalling = StringMarshalling.Utf16)]
    private static partial nint FindWindowEx(nint parent, nint after, string? className, string? title);

    [LibraryImport("user32.dll")]
    private static partial nint GetDC(nint window);

    [LibraryImport("user32.dll")]
    private static partial int ReleaseDC(nint window, nint context);

    [LibraryImport("gdi32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool BitBlt(nint target, int x, int y, int width, int height, nint source, int fromX, int fromY, uint operation);

    [LibraryImport("gdi32.dll")]
    private static partial nint CreateCompatibleDC(nint context);

    [LibraryImport("gdi32.dll")]
    private static partial nint CreateDIBSection(nint context, in BitmapHeader header, uint usage, out nint bits, nint section, uint offset);

    [LibraryImport("gdi32.dll")]
    private static partial nint SelectObject(nint context, nint item);

    [LibraryImport("gdi32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool DeleteObject(nint item);

    [LibraryImport("gdi32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool DeleteDC(nint context);
}
