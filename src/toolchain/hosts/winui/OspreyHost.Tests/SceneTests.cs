using Osprey.Host.Scenes;

namespace Osprey.Host.Tests;

/// <summary>[NATIVE-SHM-SCENE] Scene rows decode as the application wrote them, every count stays inside the rows that follow it, and a row this host cannot draw is refused whole.</summary>
public sealed class SceneTests
{
    static NativeShape Row(long code, long index = 0, double x = 0, double y = 0, double width = 0, double height = 0,
        double angle = 0, long colour = 0, long depth = 0, long textOffset = 0, long textLength = 0) =>
        new(code, index, x, y, width, height, angle, colour, depth, textOffset, textLength);

    static NativeShape Point(double x, double y) => Row(7, x: x, y: y);

    static byte[] Utf8(string text) => System.Text.Encoding.UTF8.GetBytes(text);

    static Scene Decode(NativeShape[] rows, int canvas = 0, byte[]? text = null) =>
        SceneReader.Read(rows, text ?? [], [canvas])[canvas];

    static HostException Refused(NativeShape[] rows, int canvas = 0, byte[]? text = null) =>
        Assert.Throws<HostException>(() => SceneReader.Read(rows, text ?? [], [canvas]));

    [Fact]
    public void ASceneNamesItsCanvasAndCarriesItsExtentAndZoom()
    {
        var scene = Decode([Row(20, index: 0, x: 434, y: 557, width: 0.5)]);
        Assert.Equal((434.0, 557.0, 0.5), (scene.Width, scene.Height, scene.Zoom));
        Assert.Equal((217.0, 278.5), scene.Opening);
        Assert.Empty(scene.Shapes);
        Assert.Empty(scene.Atlas);
        Assert.Empty(scene.Access);
    }

    [Fact]
    public void OnlyAZoomedSceneAsksForItsOwnSize()
    {
        Assert.Null(Decode([Row(20, x: 1728, y: 893)]).Opening);
        Assert.Null(Decode([Row(20)]).Opening);
    }

    [Fact]
    public void EveryPrimitiveDecodesInDrawingOrder()
    {
        var words = Utf8("helloAbird");
        var scene = Decode([
            Row(20, x: 100, y: 50, width: 1),
            Row(21, textOffset: 5, textLength: 1),
            Row(4, x: 1, y: 2, width: 12, colour: 0x11223344, depth: 8, textOffset: 0, textLength: 5),
            Row(0, x: 10, y: 20, width: 30, height: 40, angle: 0.5, colour: 0xFF0000FF, depth: 2),
            Row(1, x: 5, y: 6, width: 7, colour: 0x00FF00FF, depth: 1),
            Row(2, index: 2, x: 0, y: 1, width: 2.5, colour: 0x000000FF, depth: 4, height: 3),
            Point(0, 1), Point(2, 3),
            Row(3, index: 1, x: 8, y: 9, width: 10, height: 11, angle: -0.5, colour: 0xFFFFFFFF, depth: 2),
            Row(6, index: 1, x: 4, y: 14, width: 9, height: 700, colour: 0x556677FF, depth: 3, textOffset: 6, textLength: 4),
        ], text: words);
        Assert.Equal(["A"], scene.Atlas);
        Assert.Equal<Shape>(
        [
            new GlyphsShape("hello", 1, 2, 12, 0x11223344, 8),
            new RectShape(10, 20, 30, 40, 0.5, 0xFF0000FF, 2),
            new DiscShape(5, 6, 7, 0x00FF00FF, 1),
            new StripShape([(0, 1), (2, 3)], 0x000000FF, 2.5, 4),
            new SpriteShape(1, 8, 9, 10, 11, -0.5, 0xFFFFFFFF, 2),
            new LetteringShape("bird", 4, 14, 9, 0x556677FF, 3, 1, 700),
        ], scene.Shapes, ShapeComparer.Instance);
    }

    [Fact]
    public void APathDecodesItsStepsOutlinePaintAndMove()
    {
        var scene = Decode([
            Row(20, x: 10, y: 10, width: 1),
            Row(5, x: 5, y: 6, width: 2, index: 7, depth: 3),
            Row(8, index: 0, x: 1, y: 2),
            Row(8, index: 1, x: 3, y: 4),
            Row(8, index: 2, x: 5, y: 6, width: 7, height: 8),
            Row(8, index: 3, x: 1, y: 2, width: 3, height: 4, angle: 5),
            Point(6, 0),
            Row(8, index: 4, x: 9, y: 9, width: 0.5, height: 10, angle: 11, colour: 3),
            Row(8, index: 5, depth: 3, colour: 1),
            Point(0, 0), Point(4, 0), Point(4, 4),
            Row(8, index: 6),
            Row(9, index: 1, x: 2.5, depth: 2, colour: 1),
            Point(4, 0), Point(2, 0),
            Row(10, index: 1, x: 180, y: 0.4, width: 0.6, colour: 1),
            Row(11, index: 1, x: 0.6666666667, y: 1, width: 0.5, height: 1),
            Row(11, index: 0, x: 0, y: 1, width: 0, height: 1),
        ]);
        var path = Assert.IsType<PathShape>(Assert.Single(scene.Shapes));
        Assert.Equal<PathStep>(
        [
            new MoveTo(1, 2), new LineTo(3, 4), new CurveTo(5, 6, 7, 8), new CubicTo(1, 2, 3, 4, 5, 6),
            new ArcTo(9, 9, 0.5, true, true, 10, 11),
        ], path.Steps.Take(5));
        Assert.Equal([(0.0, 0.0), (4, 0), (4, 4)], Assert.IsType<PolygonStep>(path.Steps[5]).Corners);
        Assert.True(Assert.IsType<PolygonStep>(path.Steps[5]).Closed);
        Assert.IsType<CloseStep>(path.Steps[6]);
        Assert.Equal((true, 2.5, true), (path.Outline.Stroked, path.Outline.Width, path.Outline.Bevel));
        Assert.Equal([4.0, 2.0], path.Outline.Dashes);
        var paint = Assert.IsType<LinearPaint>(path.Paint);
        Assert.Equal((180.0, 0.4, 0.6, true), (paint.Angle, paint.FromAt, paint.ToAt, paint.Oklab));
        Assert.Equal(new Tint(true, 0.6666666667, 1, 0.5, 1), paint.From);
        Assert.Equal(new Tint(false, 0, 1, 0, 1), paint.To);
        Assert.Equal((5.0, 6.0, 2.0), path.Moved);
        Assert.Equal(3, path.Depth);
    }

    [Fact]
    public void AnElementBecomesAnAccessibilityNodeAndNoRoleDrawsNone()
    {
        var words = Utf8("PlayVolumeNothingSkip");
        var scene = Decode([
            Row(20, x: 100, y: 100, width: 1),
            Row(22, index: 0, x: 1, y: 2, width: 3, height: 4, angle: 0, colour: 1, textOffset: 0, textLength: 4),
            Point(0, 0),
            Row(22, index: 1, x: 5, y: 6, width: 7, height: 8, angle: 0.5, textOffset: 4, textLength: 6),
            Point(0, 1),
            Row(22, index: 3, x: 9, y: 9, width: 9, height: 9, textOffset: 10, textLength: 7),
            Point(0, 0),
            Row(22, index: 4, x: 0, y: 0, width: 100, height: 100, textOffset: 17, textLength: 4),
            Point(0, 0),
        ], text: words);
        Assert.Equal(3, scene.Access.Count);
        Assert.Equal(new Accessible("button", "Play", 1, 2, 3, 4, 0, 0, 0, true), scene.Access[0]);
        Assert.Equal(new Accessible("slider", "Volume", 5, 6, 7, 8, 0.5, 0, 1, false), scene.Access[1]);
        Assert.Equal(("menu-item", "Nothing", 9.0), (scene.Access[2].Role, scene.Access[2].Label, scene.Access[2].X));
    }

    [Fact]
    public void ASceneThatNamesNoCanvasOrOneTwiceIsRefused()
    {
        Refused([Row(20, index: 3)], canvas: 0);
        Refused([Row(20, index: -1)], canvas: 0);
        Refused([Row(20), Row(20)], canvas: 0);
        // A row of an earlier scene is refused when a second marker names no canvas, even if the
        // first is valid: the dictionary is keyed by the canvas row.
        Refused([Row(20, index: 0), Row(0), Row(20, index: 0)], canvas: 0);
    }

    [Fact]
    public void ASceneRowBeforeAnySceneIsRefused() => Refused([Row(0)]);

    [Fact]
    public void ASceneMustNameACanvasRow()
    {
        var rows = new[] { Row(20, index: 0, x: 10, y: 10, width: 1) };
        Assert.Throws<HostException>(() => SceneReader.Read(rows, [], [3]));
        Assert.Throws<HostException>(() => SceneReader.Read(rows, [], []));
        Assert.Single(SceneReader.Read(rows, [], [0]));
    }

    [Fact]
    public void ACountThatRunsPastTheRowsIsRefused()
    {
        Refused([Row(20), Row(2, index: 3), Point(0, 0)]);
        Refused([Row(20), Row(5, index: 1), Row(8, index: 0)]);
        Refused([Row(20), Row(2, index: -1)]);
    }

    [Fact]
    public void AnUnknownCodeOrValueIsRefused()
    {
        Refused([Row(20), Row(23)]);
        Refused([Row(20), Row(8, index: 7)]);
        Refused([Row(20), Row(9, index: 2)]);
        Refused([Row(20), Row(10, index: 2)]);
        Refused([Row(20), Row(11, index: 2)]);
        Refused([Row(20), Row(6, index: 3)]);
        Refused([Row(20), Row(3, index: -1)]);
        Refused([Row(20), Row(22, index: 9), Point(0, 0)]);
    }

    [Fact]
    public void ANumberThatIsNotFiniteOrAnExtentThatIsNegativeIsRefused()
    {
        Refused([Row(20, x: double.NaN)]);
        Refused([Row(20, y: -1)]);
        Refused([Row(20, width: double.PositiveInfinity)]);
        Refused([Row(20), Row(0, x: double.NaN)]);
        Refused([Row(20), Row(2, index: 1), Row(7, y: double.NaN)]);
    }

    [Fact]
    public void TextOutsideTheRunOrThatIsNotUtf8IsRefused()
    {
        var words = Utf8("Hi");
        Refused([Row(20), Row(4, textOffset: 1, textLength: 5)], text: words);
        Refused([Row(20), Row(21, textOffset: -1, textLength: 1)], text: words);
        Refused([Row(20), Row(21, textOffset: 0, textLength: -1)], text: words);
        byte[] broken = [0x41, 0xC3, 0x28, 0x42];
        Refused([Row(20), Row(21, textOffset: 0, textLength: 4)], text: broken);
        Assert.Equal(["Hi"], Decode([Row(20), Row(21, textOffset: 0, textLength: 2)], text: words).Atlas);
    }

    [Fact]
    public void ASceneWithNoRowsReadsAsNoScenes()
    {
        Assert.Empty(SceneReader.Read([], [], [0]));
        Assert.Empty(SceneReader.Read([], [], []));
        Assert.Throws<HostException>(() => SceneReader.Read([Row(0), Point(0, 0)], [], []));
    }

    // Records holding lists compare their lists by reference, so the strip's points are compared by value here.
    sealed class ShapeComparer : IEqualityComparer<Shape>
    {
        public static readonly ShapeComparer Instance = new();

        public bool Equals(Shape? left, Shape? right) => (left, right) switch
        {
            (StripShape a, StripShape b) => a.Points.SequenceEqual(b.Points) && (a.Colour, a.Stroke, a.Depth) == (b.Colour, b.Stroke, b.Depth),
            _ => Equals((object?)left, right),
        };

        public int GetHashCode(Shape shape) => shape.Depth.GetHashCode();
    }
}
