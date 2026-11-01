using Microsoft.UI;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Osprey.Host.Scenes;
using Windows.Foundation;
using Windows.System;
using Windows.UI;
using SceneShape = Osprey.Host.Scenes.Shape;
using ShapePath = Microsoft.UI.Xaml.Shapes.Path;

namespace Osprey.Host.WinUI;

/// <summary>
/// One scene on screen [WINUI-HOST-SCENE]: its shapes drawn with WinUI's own shapes on a canvas in
/// the scene's unit space at its fixed zoom from the region's top left. The elements live between frames and are reused, so a frame that moves a
/// thousand discs only moves them. Keys and the pointer reach the scene's binding in scene units.
/// </summary>
sealed class SceneView
{
    readonly ContentControl holder = new()
    {
        IsTabStop = true,
        UseSystemFocusVisuals = false,
        HorizontalContentAlignment = HorizontalAlignment.Stretch,
        VerticalContentAlignment = VerticalAlignment.Stretch,
        XYFocusKeyboardNavigation = XYFocusKeyboardNavigationMode.Disabled,
    };
    readonly Canvas canvas = new() { Background = new SolidColorBrush(Colors.Transparent) };
    readonly Dictionary<uint, SolidColorBrush> brushes = [];
    readonly Dictionary<string, ImageBrush?> atlas = [];
    readonly Action<string> report;
    IReadOnlyList<string> sources = [];
    readonly Pool<Rectangle> rects;
    readonly Pool<Ellipse> discs;
    readonly Pool<Polyline> strips;
    readonly Pool<TextBlock> glyphs;
    readonly Pool<ShapePath> paths;
    Action<object?> emit = _ => { };
    bool hovered;

    public SceneView(Action<string> report)
    {
        this.report = report;
        rects = new(canvas);
        discs = new(canvas);
        strips = new(canvas);
        glyphs = new(canvas);
        paths = new(canvas);
        holder.Content = canvas;
        canvas.PointerPressed += (_, args) => Pressed(args);
        canvas.PointerMoved += (_, args) => Moved(args);
        canvas.PointerReleased += (_, args) => Pointed(args, down: false);
        holder.KeyDown += (_, args) => Keyed(args, down: true);
        holder.KeyUp += (_, args) => Keyed(args, down: false);
        CompositionTarget.Rendering += (_, _) => hovered = false;
    }

    /// <summary>The element that holds the scene in the page.</summary>
    public FrameworkElement Element => holder;

    /// <summary>Draws <paramref name="scene"/>, reusing the shapes already on screen, and sends input to <paramref name="send"/>.</summary>
    public void Show(Scene scene, Action<object?> send)
    {
        emit = send;
        sources = scene.Atlas;
        Fitted(scene);
        foreach (var pool in new IPool[] { rects, discs, strips, glyphs, paths })
        {
            pool.Begin();
        }
        for (var order = 0; order < scene.Shapes.Count; order++)
        {
            Canvas.SetZIndex(Draw(scene.Shapes[order]), order);
        }
        foreach (var pool in new IPool[] { rects, discs, strips, glyphs, paths })
        {
            pool.End();
        }
    }

    // A scene with a size is drawn at its fixed zoom from the region's top left and clipped to itself,
    // so a larger window shows more canvas and resizes nothing; one without is in window pixels.
    void Fitted(Scene scene)
    {
        var sized = scene.Width > 0;
        canvas.Clip = sized ? new RectangleGeometry { Rect = new Rect(0, 0, scene.Width, scene.Height) } : null;
        canvas.RenderTransform = sized ? new ScaleTransform { ScaleX = scene.Zoom, ScaleY = scene.Zoom } : null;
    }

    UIElement Draw(SceneShape shape) => shape switch
    {
        RectShape rect => Quad(rects.Next(), rect.X, rect.Y, rect.W, rect.H, rect.Angle, Brush(rect.Colour)),
        SpriteShape sprite => Quad(rects.Next(), sprite.X, sprite.Y, sprite.W, sprite.H, sprite.Angle, (Brush?)Image(sprite) ?? Brush(Sprite(sprite))),
        DiscShape disc => Disc(discs.Next(), disc),
        StripShape strip => Strip(strips.Next(), strip),
        GlyphsShape text => Glyphs(glyphs.Next(), text),
        LetteringShape text => Lettering(glyphs.Next(), text),
        PathShape path => Painted(paths.Next(), path),
        _ => throw new HostException($"This host cannot draw a {shape.GetType().Name}"),
    };

    static Rectangle Quad(Rectangle quad, double x, double y, double w, double h, double angle, Brush fill)
    {
        quad.Width = Math.Max(0, w);
        quad.Height = Math.Max(0, h);
        quad.Fill = fill;
        quad.RenderTransform = angle == 0 ? null : new RotateTransform { Angle = angle * 180 / Math.PI, CenterX = w / 2, CenterY = h / 2 };
        Canvas.SetLeft(quad, x - (w / 2));
        Canvas.SetTop(quad, y - (h / 2));
        return quad;
    }

    /// <summary>
    /// The sprite's atlas image filling its quad, as the GPUI host draws it [UI-SCENE], decoded once
    /// per path from a stream, which the sandbox allows for a file in its own package. A scene with
    /// no atlas draws the tinted quad; a path that cannot be read is reported and drawn tinted.
    /// </summary>
    ImageBrush? Image(SpriteShape sprite)
    {
        if (sprite.Atlas < 0 || sprite.Atlas >= sources.Count)
        {
            return null;
        }
        var path = sources[sprite.Atlas];
        if (atlas.TryGetValue(path, out var brush))
        {
            return brush;
        }
        try
        {
            var picture = new Microsoft.UI.Xaml.Media.Imaging.BitmapImage();
            using (var file = File.OpenRead(path))
            {
                using var memory = new MemoryStream();
                file.CopyTo(memory);
                memory.Position = 0;
                picture.SetSource(System.IO.WindowsRuntimeStreamExtensions.AsRandomAccessStream(memory));
            }
            return atlas[path] = new ImageBrush { ImageSource = picture, Stretch = Stretch.Fill };
        }
        catch (Exception problem) when (problem is IOException or UnauthorizedAccessException)
        {
            report($"The sprite image {path} could not be read: {problem.Message}");
            return atlas[path] = null;
        }
    }

    // A scene without an atlas draws its sprites as tinted quads, as the GPUI host does.
    static uint Sprite(SpriteShape sprite) => sprite.Tint != 0xFFFFFFFF ? sprite.Tint : sprite.Atlas switch
    {
        0 => 0x87CEEBFF,
        1 => 0xFFA500FF,
        2 => 0x55AA55FF,
        _ => 0x888888FF,
    };

    Ellipse Disc(Ellipse disc, DiscShape shape)
    {
        disc.Width = disc.Height = Math.Max(0, shape.Radius * 2);
        disc.Fill = Brush(shape.Colour);
        Canvas.SetLeft(disc, shape.X - shape.Radius);
        Canvas.SetTop(disc, shape.Y - shape.Radius);
        return disc;
    }

    Polyline Strip(Polyline line, StripShape strip)
    {
        var points = new PointCollection();
        foreach (var (x, y) in strip.Points)
        {
            points.Add(new Point(x, y));
        }
        line.Points = points;
        line.Stroke = Brush(strip.Colour);
        line.StrokeThickness = strip.Stroke;
        return line;
    }

    TextBlock Glyphs(TextBlock block, GlyphsShape text)
    {
        block.Text = text.Text;
        block.FontSize = Math.Max(1, text.Size);
        block.FontWeight = Microsoft.UI.Text.FontWeights.Normal;
        block.Foreground = Brush(text.Colour);
        Canvas.SetLeft(block, text.X);
        Canvas.SetTop(block, text.Y);
        return block;
    }

    // Lettering's anchor moves its measured line left by none, half or all of its width.
    TextBlock Lettering(TextBlock block, LetteringShape text)
    {
        block.Text = text.Text;
        block.FontSize = Math.Max(1, text.Size);
        block.FontWeight = new Windows.UI.Text.FontWeight { Weight = (ushort)Math.Clamp(text.Weight, 1, 999) };
        block.Foreground = Brush(text.Colour);
        block.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
        Canvas.SetLeft(block, text.X - (block.DesiredSize.Width * text.Anchor / 2));
        Canvas.SetTop(block, text.Y);
        return block;
    }

    static ShapePath Painted(ShapePath shape, PathShape path)
    {
        var paint = Paints.Brush(path.Paint);
        shape.Data = Paths.Geometry(path);
        shape.Fill = path.Outline.Stroked ? null : paint;
        shape.Stroke = path.Outline.Stroked ? paint : null;
        shape.StrokeThickness = path.Outline.Stroked ? path.Outline.Width : 0;
        shape.StrokeLineJoin = path.Outline.Bevel ? PenLineJoin.Bevel : PenLineJoin.Miter;
        shape.StrokeDashArray = Dashes(path.Outline);
        return shape;
    }

    // XAML counts dashes in stroke widths; the scene counts them in its own units.
    static DoubleCollection Dashes(Outline outline)
    {
        var dashes = new DoubleCollection();
        foreach (var dash in outline.Dashes)
        {
            dashes.Add(outline.Width > 0 ? dash / outline.Width : dash);
        }
        return dashes;
    }

    SolidColorBrush Brush(uint colour)
    {
        if (!brushes.TryGetValue(colour, out var brush))
        {
            brush = new SolidColorBrush(Paints.Packed(colour));
            brushes[colour] = brush;
        }
        return brush;
    }

    void Pressed(PointerRoutedEventArgs args)
    {
        holder.Focus(FocusState.Pointer);
        canvas.CapturePointer(args.Pointer);
        Pointed(args, down: true);
    }

    // Every drag movement is sent; a hover movement at most once a frame, as the GPUI host does.
    void Moved(PointerRoutedEventArgs args)
    {
        var down = args.GetCurrentPoint(canvas).Properties.IsLeftButtonPressed;
        if (!down && hovered)
        {
            return;
        }
        hovered = !down;
        Pointed(args, down);
    }

    void Pointed(PointerRoutedEventArgs args, bool down)
    {
        var at = args.GetCurrentPoint(canvas).Position;
        emit(new PointerInput(at.X, at.Y, down, args.KeyModifiers.HasFlag(VirtualKeyModifiers.Shift)));
        if (!down)
        {
            canvas.ReleasePointerCapture(args.Pointer);
        }
        args.Handled = true;
    }

    /// <summary>Sends a key going down and up, as the keyboard does for one press.</summary>
    public void Press(string key)
    {
        emit(new KeyInput(key, true));
        emit(new KeyInput(key, false));
    }

    /// <summary>Sends the pointer going down and up at a point in scene units, as one click there does.</summary>
    public void Tap(double x, double y)
    {
        emit(new PointerInput(x, y, true, false));
        emit(new PointerInput(x, y, false, false));
    }

    void Keyed(KeyRoutedEventArgs args, bool down)
    {
        if (Keys.Name(args.Key) is { } name)
        {
            emit(new KeyInput(name, down));
            args.Handled = true;
        }
    }
}

/// <summary>The scene key vocabulary [UI-SCENE]: arrows and a few named keys, and single lowercase letters and digits.</summary>
static class Keys
{
    /// <summary>The framework's name for <paramref name="key"/>, or null for a key scenes do not take.</summary>
    public static string? Name(VirtualKey key) => key switch
    {
        VirtualKey.Left => "left",
        VirtualKey.Right => "right",
        VirtualKey.Up => "up",
        VirtualKey.Down => "down",
        VirtualKey.Space => "space",
        VirtualKey.Enter => "enter",
        VirtualKey.Escape => "escape",
        VirtualKey.Tab => "tab",
        VirtualKey.Back => "backspace",
        >= VirtualKey.A and <= VirtualKey.Z => ((char)('a' + (key - VirtualKey.A))).ToString(),
        >= VirtualKey.Number0 and <= VirtualKey.Number9 => ((char)('0' + (key - VirtualKey.Number0))).ToString(),
        _ => null,
    };
}

/// <summary>Elements of one kind kept on a canvas between frames: a frame takes what it needs and hides the rest.</summary>
interface IPool
{
    void Begin();

    void End();
}

sealed class Pool<T>(Canvas canvas) : IPool where T : UIElement, new()
{
    readonly List<T> items = [];
    int used;

    public void Begin() => used = 0;

    public T Next()
    {
        if (used == items.Count)
        {
            var made = new T();
            items.Add(made);
            canvas.Children.Add(made);
        }
        var item = items[used++];
        item.Visibility = Visibility.Visible;
        return item;
    }

    public void End()
    {
        for (var index = used; index < items.Count; index++)
        {
            items[index].Visibility = Visibility.Collapsed;
        }
    }
}
