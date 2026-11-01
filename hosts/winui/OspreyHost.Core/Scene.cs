using System.Text.Json;

namespace Osprey.Host.Scenes;

/// <summary>One drawn primitive in scene units, with the depth that orders it [UI-SCENE].</summary>
public abstract record Shape(double Depth);

/// <summary>A rectangle centred on its point, turned by <paramref name="Angle"/> radians.</summary>
public sealed record RectShape(double X, double Y, double W, double H, double Angle, uint Colour, double Depth) : Shape(Depth);

/// <summary>A disc centred on its point.</summary>
public sealed record DiscShape(double X, double Y, double Radius, uint Colour, double Depth) : Shape(Depth);

/// <summary>An open line through its points, <paramref name="Stroke"/> units wide.</summary>
public sealed record StripShape(IReadOnlyList<(double X, double Y)> Points, uint Colour, double Stroke, double Depth) : Shape(Depth);

/// <summary>Atlas image <paramref name="Atlas"/>, centred on its point and tinted.</summary>
public sealed record SpriteShape(int Atlas, double X, double Y, double W, double H, double Angle, uint Tint, double Depth) : Shape(Depth);

/// <summary>One line of text whose top-left corner is its point.</summary>
public sealed record GlyphsShape(string Text, double X, double Y, double Size, uint Colour, double Depth) : Shape(Depth);

/// <summary>A vector path, filled or stroked with a paint, after its own move and scale.</summary>
/// <summary>Text in the system font at a weight, its left edge, centre or right edge (anchor 0, 1 or 2) at x and the top of its line at y.</summary>
public sealed record LetteringShape(string Text, double X, double Y, double Size, uint Colour, double Depth, int Anchor, double Weight) : Shape(Depth);

public sealed record PathShape(IReadOnlyList<PathStep> Steps, Outline Outline, Paint Paint, (double X, double Y, double Scale)? Moved, double Depth) : Shape(Depth);

/// <summary>One path-builder command in scene units.</summary>
public abstract record PathStep;

/// <summary>Starts a figure at a point.</summary>
public sealed record MoveTo(double X, double Y) : PathStep;

/// <summary>A straight line to a point.</summary>
public sealed record LineTo(double X, double Y) : PathStep;

/// <summary>A quadratic curve to a point through one control point.</summary>
public sealed record CurveTo(double X, double Y, double ControlX, double ControlY) : PathStep;

/// <summary>A cubic curve to a point through two control points.</summary>
public sealed record CubicTo(double X, double Y, double AX, double AY, double BX, double BY) : PathStep;

/// <summary>An elliptical arc to a point, as SVG draws it.</summary>
public sealed record ArcTo(double RadiusX, double RadiusY, double Rotation, bool Large, bool Sweep, double X, double Y) : PathStep;

/// <summary>A polygon through its corners, closed or open.</summary>
public sealed record Polygon(IReadOnlyList<(double X, double Y)> Corners, bool Closed) : PathStep;

/// <summary>Closes the current figure.</summary>
public sealed record Close : PathStep;

/// <summary>A fill, or a stroke with its width, bevel or miter joins and dash pattern.</summary>
public sealed record Outline(bool Stroked, double Width, bool Bevel, IReadOnlyList<double> Dashes);

/// <summary>A colour with float channels, sRGB or hue (in turns), saturation and lightness, and its opacity.</summary>
public readonly record struct Tint(bool Hsl, double First, double Second, double Third, double Alpha);

/// <summary>A solid tint or a two-stop linear gradient whose angle follows CSS, in degrees.</summary>
public abstract record Paint;

/// <summary>One tint everywhere.</summary>
public sealed record SolidPaint(Tint Tint) : Paint;

/// <summary>A gradient from one tint at <paramref name="FromAt"/> to another at <paramref name="ToAt"/>, blended in Oklab when asked.</summary>
public sealed record LinearPaint(double Angle, Tint From, double FromAt, Tint To, double ToAt, bool Oklab) : Paint;

/// <summary>One decoded scene: its unit space (zero for window pixels), its zoom in pixels per unit (0 fits it to its region), its atlas and its shapes, back to front.</summary>
public sealed record Scene(double Width, double Height, double Zoom, IReadOnlyList<string> Atlas, IReadOnlyList<Shape> Shapes)
{
    /// <summary>
    /// The size, in window units, of the window an application that is this one scene opens at: its
    /// units times its zoom. A scene of window pixels or at zoom 0 fits whatever region holds it, so it
    /// asks for no size.
    /// </summary>
    public (double Width, double Height)? Opening => Width > 0 && Height > 0 && Zoom > 0 ? (Width * Zoom, Height * Zoom) : null;
}

/// <summary>
/// Decodes a scene node and the primitives streamed for it [UI-SCENE-STREAM] into shapes ordered
/// back to front, stably by depth, exactly as the encoder wrote them: an array per primitive with
/// its type code first. Anything malformed is refused, never drawn as a guess.
/// </summary>
public static class SceneDecoder
{
    /// <summary>The scene <paramref name="node"/> describes, with <paramref name="streamed"/> after its own shapes at equal depth.</summary>
    public static Scene Decode(JsonElement node, IReadOnlyList<Streamed> streamed)
    {
        var width = Real(Envelopes.Member(node, "width"), "width");
        var height = Real(Envelopes.Member(node, "height"), "height");
        if ((width == 0) != (height == 0))
        {
            throw new HostException("A window-pixel scene has zero width and zero height");
        }
        var zoom = node.TryGetProperty("zoom", out var given) ? Real(given, "zoom") : 1;
        if (zoom < 0)
        {
            throw new HostException("A scene's zoom must be zero or positive");
        }
        var shapes = Envelopes.Items(node, "shapes", Shape).Concat(streamed.Select(Stream));
        return new Scene(width, height, zoom, Envelopes.Items(node, "atlas", item => item.GetString() ?? ""), [.. shapes.OrderBy(shape => shape.Depth)]);
    }

    static Shape Stream(Streamed item) => item.Code switch
    {
        0 => new RectShape(item.A, item.B, item.C, item.D, item.E, Colour(item.Colour), item.Depth),
        1 => new DiscShape(item.A, item.B, item.C, Colour(item.Colour), item.Depth),
        3 => new SpriteShape(checked((int)item.Index), item.A, item.B, item.C, item.D, item.E, Colour(item.Colour), item.Depth),
        var code => throw new HostException($"Unknown streamed shape code {code}"),
    };

    static Shape Shape(JsonElement item)
    {
        var values = Values(item, "A shape");
        return Code(values) switch
        {
            0 => Exactly(values, 8, () => new RectShape(Real(values[1]), Real(values[2]), Real(values[3]), Real(values[4]), Real(values[5]), Colour(values[6]), Real(values[7]))),
            1 => Exactly(values, 6, () => new DiscShape(Real(values[1]), Real(values[2]), Real(values[3]), Colour(values[4]), Real(values[5]))),
            2 => Strip(values),
            3 => Exactly(values, 9, () => new SpriteShape((int)Whole(values[1]), Real(values[2]), Real(values[3]), Real(values[4]), Real(values[5]), Real(values[6]), Colour(values[7]), Real(values[8]))),
            4 => Exactly(values, 7, () => new GlyphsShape(Line(values[1]), Real(values[2]), Real(values[3]), Real(values[4]), Colour(values[5]), Real(values[6]))),
            5 => Exactly(values, 6, () => Path(values)),
            6 => Exactly(values, 9, () => Lettering(values)),
            var code => throw new HostException($"Unknown shape code {code}"),
        };
    }

    static StripShape Strip(IReadOnlyList<JsonElement> values)
    {
        var count = Whole(values.Count > 1 ? values[1] : default);
        if (count < 0 || values.Count != 5 + (count * 2))
        {
            throw new HostException($"A strip of {count} points has {values.Count} values");
        }
        var tail = 2 + ((int)count * 2);
        var points = Enumerable.Range(0, (int)count).Select(index => (Real(values[2 + (index * 2)]), Real(values[3 + (index * 2)]))).ToList();
        return new StripShape(points, Colour(values[tail]), Real(values[tail + 1]), Real(values[tail + 2]));
    }

    static PathShape Path(IReadOnlyList<JsonElement> values)
    {
        var steps = Values(values[1], "Path steps").Select(Step).ToList();
        var moved = Values(values[4], "A path's move");
        (double, double, double)? transform = moved.Count switch
        {
            0 => null,
            3 => (Real(moved[0]), Real(moved[1]), Real(moved[2])),
            _ => throw new HostException("A path's move is x, y and scale"),
        };
        return new PathShape(steps, Outlined(Values(values[2], "An outline")), Painted(Values(values[3], "A paint")), transform, Real(values[5]));
    }

    static PathStep Step(JsonElement item)
    {
        var values = Values(item, "A path step");
        return Code(values) switch
        {
            0 => Exactly(values, 3, () => new MoveTo(Real(values[1]), Real(values[2]))),
            1 => Exactly(values, 3, () => new LineTo(Real(values[1]), Real(values[2]))),
            2 => Exactly(values, 5, () => new CurveTo(Real(values[1]), Real(values[2]), Real(values[3]), Real(values[4]))),
            3 => Exactly(values, 7, () => new CubicTo(Real(values[1]), Real(values[2]), Real(values[3]), Real(values[4]), Real(values[5]), Real(values[6]))),
            4 => Exactly(values, 8, () => new ArcTo(Real(values[1]), Real(values[2]), Real(values[3]), Whole(values[4]) != 0, Whole(values[5]) != 0, Real(values[6]), Real(values[7]))),
            5 => Corners(values),
            6 => Exactly(values, 1, () => new Close()),
            var code => throw new HostException($"Unknown path step code {code}"),
        };
    }

    static Polygon Corners(IReadOnlyList<JsonElement> values)
    {
        if (values.Count < 2 || (values.Count - 2) % 2 != 0)
        {
            throw new HostException("A polygon is a closed flag and corner pairs");
        }
        var corners = Enumerable.Range(0, (values.Count - 2) / 2).Select(index => (Real(values[2 + (index * 2)]), Real(values[3 + (index * 2)]))).ToList();
        return new Polygon(corners, Whole(values[1]) != 0);
    }

    static Outline Outlined(IReadOnlyList<JsonElement> values) => Code(values) switch
    {
        0 => new Outline(false, 0, false, []),
        1 when values.Count >= 3 => new Outline(true, Real(values[1]), Whole(values[2]) != 0, [.. values.Skip(3).Select(dash => Real(dash))]),
        _ => throw new HostException("An outline is a fill, or a stroke with its width and joins"),
    };

    static Paint Painted(IReadOnlyList<JsonElement> values) => Code(values) switch
    {
        0 => Exactly(values, 2, () => (Paint)new SolidPaint(Tinted(values[1]))),
        1 => Exactly(values, 7, () => (Paint)new LinearPaint(Real(values[1]), Tinted(values[2]), Real(values[3]), Tinted(values[4]), Real(values[5]), Whole(values[6]) != 0)),
        var code => throw new HostException($"Unknown paint code {code}"),
    };

    static Tint Tinted(JsonElement item)
    {
        var values = Values(item, "A tint");
        return Exactly(values, 5, () => Code(values) switch
        {
            0 => new Tint(false, Real(values[1]), Real(values[2]), Real(values[3]), Real(values[4])),
            1 => new Tint(true, Real(values[1]), Real(values[2]), Real(values[3]), Real(values[4])),
            var code => throw new HostException($"Unknown tint code {code}"),
        });
    }

    static T Exactly<T>(IReadOnlyList<JsonElement> values, int count, Func<T> make) =>
        values.Count == count ? make() : throw new HostException($"A scene primitive has {values.Count} values where it needs {count}");

    static IReadOnlyList<JsonElement> Values(JsonElement item, string what) =>
        item.ValueKind == JsonValueKind.Array ? [.. item.EnumerateArray()] : throw new HostException($"{what} is not an array");

    static LetteringShape Lettering(IReadOnlyList<JsonElement> values)
    {
        var anchor = Whole(values[7]);
        return anchor is >= 0 and <= 2
            ? new LetteringShape(Line(values[1]), Real(values[2]), Real(values[3]), Real(values[4]), Colour(values[5]), Real(values[6]), (int)anchor, Real(values[8]))
            : throw new HostException($"A lettering anchor is 0, 1 or 2, not {anchor}");
    }

    static long Code(IReadOnlyList<JsonElement> values) => values.Count > 0 ? Whole(values[0]) : throw new HostException("A scene primitive is empty");

    static long Whole(JsonElement item) =>
        item.ValueKind == JsonValueKind.Number && item.TryGetInt64(out var whole) ? whole : throw new HostException($"A scene value is {item.ValueKind}, not a whole number");

    static double Real(JsonElement item) => Real(item, "value");

    static double Real(JsonElement item, string what) =>
        item.ValueKind == JsonValueKind.Number && item.TryGetDouble(out var real) && double.IsFinite(real) ? real : throw new HostException($"A scene's {what} is not a finite number");

    static uint Colour(JsonElement item) => Colour(Whole(item));

    static uint Colour(long packed) =>
        packed is >= 0 and <= uint.MaxValue ? (uint)packed : throw new HostException($"Colour {packed} is not 0xRRGGBBAA");

    static string Line(JsonElement item) =>
        item.GetString() is { } text && !text.Contains('\n') ? text : throw new HostException("Scene text is one line");
}
