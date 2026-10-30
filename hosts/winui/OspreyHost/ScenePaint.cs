using Microsoft.UI.Xaml.Media;
using Osprey.Host.Scenes;
using Windows.Foundation;
using Windows.UI;

namespace Osprey.Host.WinUI;

/// <summary>
/// Scene colours and paints as WinUI brushes [WINUI-HOST-SCENE]: packed 0xRRGGBBAA colours, sRGB
/// and HSL tints with the hue in turns, and two-stop linear gradients whose angle follows CSS,
/// blended in Oklab when the scene asks, as the GPUI host draws them.
/// </summary>
static class Paints
{
    // An Oklab blend is drawn as this many sRGB stops between the two tints.
    const int OklabStops = 12;

    /// <summary>The colour a packed 0xRRGGBBAA value names.</summary>
    public static Color Packed(uint colour) =>
        Color.FromArgb((byte)(colour & 0xFF), (byte)(colour >> 24), (byte)(colour >> 16), (byte)(colour >> 8));

    /// <summary>A brush for a paint.</summary>
    public static Brush Brush(Paint paint) => paint switch
    {
        SolidPaint solid => new SolidColorBrush(Colour(solid.Tint)),
        LinearPaint linear => Gradient(linear),
        _ => throw new HostException($"This host cannot paint with a {paint.GetType().Name}"),
    };

    /// <summary>The colour of a tint: sRGB channels, or hue in turns, saturation and lightness.</summary>
    public static Color Colour(Tint tint)
    {
        var (red, green, blue) = tint.Hsl ? FromHsl(tint.First, tint.Second, tint.Third) : (tint.First, tint.Second, tint.Third);
        return Color.FromArgb(Byte(tint.Alpha), Byte(red), Byte(green), Byte(blue));
    }

    static byte Byte(double channel) => (byte)Math.Round(Math.Clamp(channel, 0, 1) * 255);

    static (double, double, double) FromHsl(double hue, double saturation, double lightness)
    {
        var chroma = (1 - Math.Abs((2 * lightness) - 1)) * saturation;
        var sector = (((hue % 1) + 1) % 1) * 6;
        var second = chroma * (1 - Math.Abs((sector % 2) - 1));
        var (r, g, b) = (int)sector switch
        {
            0 => (chroma, second, 0.0),
            1 => (second, chroma, 0.0),
            2 => (0.0, chroma, second),
            3 => (0.0, second, chroma),
            4 => (second, 0.0, chroma),
            _ => (chroma, 0.0, second),
        };
        var lift = lightness - (chroma / 2);
        return (r + lift, g + lift, b + lift);
    }

    // CSS angles: 0 degrees runs bottom to top, 90 left to right, across the shape's bounding box.
    static LinearGradientBrush Gradient(LinearPaint linear)
    {
        var radians = linear.Angle * Math.PI / 180;
        var (dx, dy) = (Math.Sin(radians) / 2, -Math.Cos(radians) / 2);
        var gradient = new LinearGradientBrush { StartPoint = new Point(0.5 - dx, 0.5 - dy), EndPoint = new Point(0.5 + dx, 0.5 + dy) };
        var (from, to) = (Colour(linear.From), Colour(linear.To));
        var steps = linear.Oklab ? OklabStops : 1;
        for (var step = 0; step <= steps; step++)
        {
            var share = (double)step / steps;
            var colour = linear.Oklab ? Oklab.Mix(from, to, share) : (step == 0 ? from : to);
            gradient.GradientStops.Add(new GradientStop { Color = colour, Offset = linear.FromAt + ((linear.ToAt - linear.FromAt) * share) });
        }
        return gradient;
    }
}

/// <summary>Colour mixing in Oklab, the perceptual space a gradient may ask to be blended in.</summary>
static class Oklab
{
    /// <summary>The colour <paramref name="share"/> of the way from <paramref name="from"/> to <paramref name="to"/>, mixed in Oklab.</summary>
    public static Color Mix(Color from, Color to, double share)
    {
        var (l1, a1, b1) = ToLab(from);
        var (l2, a2, b2) = ToLab(to);
        var (r, g, b) = FromLab(l1 + ((l2 - l1) * share), a1 + ((a2 - a1) * share), b1 + ((b2 - b1) * share));
        var alpha = from.A + ((to.A - from.A) * share);
        return Color.FromArgb((byte)Math.Round(alpha), Encode(r), Encode(g), Encode(b));
    }

    static double Decode(byte channel)
    {
        var value = channel / 255.0;
        return value <= 0.04045 ? value / 12.92 : Math.Pow((value + 0.055) / 1.055, 2.4);
    }

    static byte Encode(double linear)
    {
        var value = linear <= 0.0031308 ? linear * 12.92 : (1.055 * Math.Pow(linear, 1 / 2.4)) - 0.055;
        return (byte)Math.Round(Math.Clamp(value, 0, 1) * 255);
    }

    static (double, double, double) ToLab(Color colour)
    {
        var (r, g, b) = (Decode(colour.R), Decode(colour.G), Decode(colour.B));
        var l = Math.Cbrt((0.4122214708 * r) + (0.5363325363 * g) + (0.0514459929 * b));
        var m = Math.Cbrt((0.2119034982 * r) + (0.6806995451 * g) + (0.1073969566 * b));
        var s = Math.Cbrt((0.0883024619 * r) + (0.2817188376 * g) + (0.6299787005 * b));
        return ((0.2104542553 * l) + (0.7936177850 * m) - (0.0040720468 * s), (1.9779984951 * l) - (2.4285922050 * m) + (0.4505937099 * s), (0.0259040371 * l) + (0.7827717662 * m) - (0.8086757660 * s));
    }

    static (double, double, double) FromLab(double lightness, double a, double b)
    {
        var l = Math.Pow(lightness + (0.3963377774 * a) + (0.2158037573 * b), 3);
        var m = Math.Pow(lightness - (0.1055613458 * a) - (0.0638541728 * b), 3);
        var s = Math.Pow(lightness - (0.0894841775 * a) - (1.2914855480 * b), 3);
        return ((4.0767416621 * l) - (3.3077115913 * m) + (0.2309699292 * s), (-1.2684380046 * l) + (2.6097574011 * m) - (0.3413193965 * s), (-0.0041960863 * l) - (0.7034186147 * m) + (1.7076147010 * s));
    }
}

/// <summary>Scene paths as WinUI geometry: figures, lines, curves, SVG arcs and polygons, then the path's own move and scale.</summary>
static class Paths
{
    /// <summary>The geometry of <paramref name="path"/>; its move and scale shape the geometry, not the stroke.</summary>
    public static PathGeometry Geometry(PathShape path)
    {
        var geometry = new PathGeometry();
        PathFigure? figure = null;
        foreach (var step in path.Steps)
        {
            figure = Add(geometry, figure, step);
        }
        if (path.Moved is var (x, y, scale))
        {
            var moved = new TransformGroup();
            moved.Children.Add(new TranslateTransform { X = x, Y = y });
            moved.Children.Add(new ScaleTransform { ScaleX = scale, ScaleY = scale });
            geometry.Transform = moved;
        }
        return geometry;
    }

    // Returns the figure the next step continues; a closed figure is continued by a new one from its start.
    static PathFigure? Add(PathGeometry geometry, PathFigure? figure, PathStep step)
    {
        switch (step)
        {
            case MoveTo move:
                return Begin(geometry, move.X, move.Y);
            case Polygon polygon:
                Outlined(geometry, polygon);
                return null;
            case Close when figure is not null:
                figure.IsClosed = true;
                return Begin(geometry, figure.StartPoint.X, figure.StartPoint.Y);
            case Close:
                return null;
            default:
                figure ??= Begin(geometry, 0, 0);
                figure.Segments.Add(Segment(step));
                return figure;
        }
    }

    static PathFigure Begin(PathGeometry geometry, double x, double y)
    {
        var figure = new PathFigure { StartPoint = new Point(x, y), IsFilled = true };
        geometry.Figures.Add(figure);
        return figure;
    }

    static PathSegment Segment(PathStep step) => step switch
    {
        LineTo line => new LineSegment { Point = new Point(line.X, line.Y) },
        CurveTo curve => new QuadraticBezierSegment { Point1 = new Point(curve.ControlX, curve.ControlY), Point2 = new Point(curve.X, curve.Y) },
        CubicTo cubic => new BezierSegment { Point1 = new Point(cubic.AX, cubic.AY), Point2 = new Point(cubic.BX, cubic.BY), Point3 = new Point(cubic.X, cubic.Y) },
        ArcTo arc => new ArcSegment
        {
            Point = new Point(arc.X, arc.Y),
            Size = new Size(Math.Abs(arc.RadiusX), Math.Abs(arc.RadiusY)),
            RotationAngle = arc.Rotation,
            IsLargeArc = arc.Large,
            SweepDirection = arc.Sweep ? SweepDirection.Clockwise : SweepDirection.Counterclockwise,
        },
        _ => throw new HostException($"This host cannot draw a {step.GetType().Name} step"),
    };

    static void Outlined(PathGeometry geometry, Polygon polygon)
    {
        if (polygon.Corners.Count == 0)
        {
            return;
        }
        var figure = Begin(geometry, polygon.Corners[0].X, polygon.Corners[0].Y);
        foreach (var (x, y) in polygon.Corners.Skip(1))
        {
            figure.Segments.Add(new LineSegment { Point = new Point(x, y) });
        }
        figure.IsClosed = polygon.Closed;
    }
}
