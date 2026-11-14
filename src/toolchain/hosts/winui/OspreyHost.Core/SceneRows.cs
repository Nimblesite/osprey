namespace Osprey.Host.Scenes;

/// <summary>One drawn primitive in scene units, with the depth that orders it [NATIVE-SHM-SCENE].</summary>
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

/// <summary>Text in the system font at a weight, its left edge, centre or right edge (anchor 0, 1 or 2) at x and the top of its line at y.</summary>
public sealed record LetteringShape(string Text, double X, double Y, double Size, uint Colour, double Depth, int Anchor, double Weight) : Shape(Depth);

/// <summary>A vector path, filled or stroked with a paint, after its own move and scale.</summary>
public sealed record PathShape(IReadOnlyList<PathStep> Steps, Outline Outline, Paint Paint, (double X, double Y, double Scale)? Moved, double Depth) : Shape(Depth);

/// <summary>One path-builder command in scene units.</summary>
public abstract record PathStep;

/// <summary>Starts a figure at a point.</summary>
public sealed record MoveTo(double X, double Y) : PathStep;

/// <summary>A straight line to a point.</summary>
public sealed record LineTo(double X, double Y) : PathStep;

/// <summary>A quadratic curve to a point through one control point.</summary>
public sealed record CurveTo(double X, double Y, double ControlX, double ControlY) : PathStep;

/// <summary>A cubic curve to a point through two control points and a third point.</summary>
public sealed record CubicTo(double X, double Y, double AX, double AY, double BX, double BY) : PathStep;

/// <summary>An elliptical arc to a point, as SVG draws it.</summary>
public sealed record ArcTo(double RadiusX, double RadiusY, double Rotation, bool Large, bool Sweep, double X, double Y) : PathStep;

/// <summary>A polygon through its corners, closed or open.</summary>
public sealed record PolygonStep(IReadOnlyList<(double X, double Y)> Corners, bool Closed) : PathStep;

/// <summary>Closes the current figure.</summary>
public sealed record CloseStep : PathStep;

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

/// <summary>One element of a scene's accessibility tree [UI-MACHINE-A11Y]: bounds in scene units and, for a slider, its value and range.</summary>
public sealed record Accessible(string Role, string Label, double X, double Y, double W, double H, double Value, double Min, double Max, bool Selected);

/// <summary>One checked scene: its unit space (zero for window pixels), its zoom in pixels per unit (0 fits it to its region), its atlas and its shapes.</summary>
public sealed record Scene(double Width, double Height, double Zoom, IReadOnlyList<string> Atlas, IReadOnlyList<Shape> Shapes, IReadOnlyList<Accessible> Access)
{
    /// <summary>
    /// The size, in window units, of the window an application that is this one scene opens at: its
    /// units times its zoom. A scene of window pixels or at zoom 0 fits whatever region holds it, so it
    /// asks for no size.
    /// </summary>
    public (double Width, double Height)? Opening => Width > 0 && Height > 0 && Zoom > 0 ? (Width * Zoom, Height * Zoom) : null;
}

/// <summary>
/// Checks and reads a frame's scene rows [NATIVE-SHM-SCENE] exactly as the reference host does:
/// code 20 opens a scene naming a canvas row, the rows after it belong to that canvas until the next
/// 20, and every count must stay inside the rows that follow it. A row this host cannot draw is
/// refused whole, never guessed.
/// </summary>
public static class SceneReader
{
    const long Point = 7;

    /// <summary>Every scene, by the canvas row its marker names.</summary>
    public static IReadOnlyDictionary<int, Scene> Read(ReadOnlySpan<NativeShape> rows, ReadOnlySpan<byte> text, IReadOnlyList<int> canvases)
    {
        var all = new Dictionary<int, Growing>();
        var at = new Cursor(rows, text);
        Growing? current = null;
        while (at.Next < rows.Length)
        {
            var head = rows[at.Next];
            FrameReader.Finite(head, at.Next);
            at.Next++;
            if (head.Code == 20)
            {
                if (head.Index < 0 || head.Index > int.MaxValue || !canvases.Contains((int)head.Index) || all.ContainsKey((int)head.Index))
                {
                    throw new HostException("A scene names no canvas row, or one twice");
                }
                var row = (int)head.Index;
                current = new Growing(FrameReader.Extent(head.X, row), FrameReader.Extent(head.Y, row), FrameReader.Extent(head.Width, row));
                all[row] = current;
                continue;
            }
            var scene = current ?? throw new HostException("A scene row before any scene");
            switch (head.Code)
            {
                case 21:
                    scene.Atlas.Add(at.Text(head));
                    break;
                case 22:
                    if (Element(ref at, head) is { } element)
                    {
                        scene.Access.Add(element);
                    }
                    break;
                default:
                    scene.Shapes.Add(Drawn(ref at, head));
                    break;
            }
        }
        return all.ToDictionary(pair => pair.Key, pair => pair.Value.Done());
    }

    static Shape Drawn(ref Cursor at, NativeShape head)
    {
        var depth = head.Depth;
        return head.Code switch
        {
            0 => new RectShape(head.X, head.Y, head.Width, head.Height, head.Angle, Tinted(head), depth),
            1 => new DiscShape(head.X, head.Y, head.Width, Tinted(head), depth),
            2 => new StripShape(at.Points(head.Index), Tinted(head), head.Width, depth),
            3 => new SpriteShape(Index(head.Index, "A sprite index below zero"), head.X, head.Y, head.Width, head.Height, head.Angle, Tinted(head), depth),
            4 => new GlyphsShape(at.Text(head), head.X, head.Y, head.Width, Tinted(head), depth),
            5 => Path(ref at, head),
            6 => new LetteringShape(at.Text(head), head.X, head.Y, head.Width, Tinted(head), depth, Anchor(head.Index), head.Height),
            var code => throw new HostException($"An unknown scene code {code}"),
        };
    }

    static PathShape Path(ref Cursor at, NativeShape head)
    {
        var count = at.Count(head.Index);
        var steps = new List<PathStep>(count);
        for (var step = 0; step < count; step++)
        {
            steps.Add(Step(ref at));
        }
        var moved = (head.X, head.Y, head.Width);
        return new PathShape(steps, Outlined(ref at), Painted(ref at), moved == (0, 0, 1) ? null : moved, head.Depth);
    }

    static PathStep Step(ref Cursor at)
    {
        var row = at.Take(8);
        return row.Index switch
        {
            0 => new MoveTo(row.X, row.Y),
            1 => new LineTo(row.X, row.Y),
            2 => new CurveTo(row.X, row.Y, row.Width, row.Height),
            3 => Cubic(ref at, row),
            4 => new ArcTo(row.X, row.Y, row.Width, (row.Colour & 1) != 0, (row.Colour & 2) != 0, row.Height, row.Angle),
            5 => new PolygonStep(at.Points(row.Depth), (row.Colour & 1) != 0),
            6 => new CloseStep(),
            var index => throw new HostException($"An unknown path step {index}"),
        };
    }

    static CubicTo Cubic(ref Cursor at, NativeShape row)
    {
        var last = at.Take(Point);
        return new CubicTo(row.X, row.Y, row.Width, row.Height, row.Angle, last.X);
    }

    static Outline Outlined(ref Cursor at)
    {
        var row = at.Take(9);
        return row.Index switch
        {
            0 => new Outline(false, 0, false, []),
            1 => new Outline(true, row.X, (row.Colour & 1) != 0, [.. at.Points(row.Depth).Select(point => point.X)]),
            var index => throw new HostException($"An unknown outline {index}"),
        };
    }

    static Paint Painted(ref Cursor at)
    {
        var row = at.Take(10);
        return row.Index switch
        {
            0 => new SolidPaint(Tint(ref at)),
            1 => new LinearPaint(row.X, Tint(ref at), row.Y, Tint(ref at), row.Width, (row.Colour & 1) != 0),
            var index => throw new HostException($"An unknown paint {index}"),
        };
    }

    static Tint Tint(ref Cursor at)
    {
        var row = at.Take(11);
        return row.Index switch
        {
            0 => new Tint(false, row.X, row.Y, row.Width, row.Height),
            1 => new Tint(true, row.X, row.Y, row.Width, row.Height),
            var index => throw new HostException($"An unknown tint {index}"),
        };
    }

    static Accessible? Element(ref Cursor at, NativeShape head)
    {
        var range = at.Take(Point);
        var role = head.Index switch
        {
            0 or 2 => "button",
            1 => "slider",
            3 => "menu-item",
            4 => null,
            _ => throw new HostException("An unknown accessible role"),
        };
        var label = at.Text(head);
        // An element with no name cannot be announced, exactly as the reference host's reader skips it.
        return role is null || label.Length == 0
            ? null
            : new Accessible(role, label, head.X, head.Y, head.Width, head.Height, head.Angle, range.X, range.Y, (head.Colour & 1) != 0);
    }

    static int Anchor(long index) =>
        index is >= 0 and <= 2 ? (int)index : throw new HostException("An unknown lettering anchor");

    static int Index(long value, string problem) =>
        value is >= 0 and <= int.MaxValue ? (int)value : throw new HostException(problem);

    static uint Tinted(NativeShape row) => FrameReader.Colour(row.Colour, 0);

    /// <summary>A scene while its rows arrive; it becomes a read-only <see cref="Scene"/> once its run ends.</summary>
    sealed class Growing(double width, double height, double zoom)
    {
        public List<string> Atlas { get; } = [];

        public List<Shape> Shapes { get; } = [];

        public List<Accessible> Access { get; } = [];

        public Scene Done() => new(width, height, zoom, Atlas, Shapes, Access);
    }

    /// <summary>Scene rows in order, with the next one taken on demand; every count is checked against what is left.</summary>
    ref struct Cursor(ReadOnlySpan<NativeShape> rows, ReadOnlySpan<byte> text)
    {
        readonly ReadOnlySpan<NativeShape> rows = rows;
        readonly ReadOnlySpan<byte> text = text;

        /// <summary>The next row to read.</summary>
        public int Next;

        /// <summary>Takes the next row, which must have <paramref name="code"/> and finite values.</summary>
        public NativeShape Take(long code)
        {
            if (Next >= rows.Length || rows[Next].Code != code)
            {
                throw new HostException($"Scene row {Next} should have code {code}");
            }
            FrameReader.Finite(rows[Next], Next);
            return rows[Next++];
        }

        /// <summary>A count that must stay inside the rows that follow it.</summary>
        public int Count(long value) =>
            value is >= 0 && value <= rows.Length - Next
                ? (int)value
                : throw new HostException("A scene count that runs past its rows");

        /// <summary><paramref name="count"/> point rows.</summary>
        public List<(double X, double Y)> Points(long count)
        {
            var wanted = Count(count);
            var points = new List<(double, double)>(wanted);
            for (var index = 0; index < wanted; index++)
            {
                var row = Take(Point);
                points.Add((row.X, row.Y));
            }
            return points;
        }

        /// <summary>The text a row names, checked inside the text run on UTF-8 boundaries.</summary>
        public string Text(NativeShape row) => FrameReader.Text(text, row.TextOffset, row.TextLength, Next);
    }
}
