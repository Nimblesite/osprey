using System.Text.Json;
using Osprey.Host.Scenes;

namespace Osprey.Host.Tests;

/// <summary>[UI-SCENE] Scene nodes decode as the encoder wrote them, in a stable depth order, with streamed shapes after the node's own [UI-SCENE-STREAM].</summary>
public sealed class SceneTests
{
    static Scene Decode(string node, params Streamed[] streamed) => SceneDecoder.Decode(JsonDocument.Parse(node).RootElement.Clone(), streamed);

    [Fact]
    public void EveryPrimitiveDecodesAndEqualDepthsKeepTheirOrder()
    {
        var scene = Decode("""{"width":100,"height":50,"atlas":["bird.png","hawk.png"],"shapes":[[4,"hello",1,2,12,287454020,8],[0,10,20,30,40,0.5,4278190335,2],[1,5,6,7,16711935,1],[2,2,0,1,2,3,255,2.5,4],[3,1,8,9,10,11,-0.5,4294967295,2]]}""");
        Assert.Equal((100.0, 50.0), (scene.Width, scene.Height));
        Assert.Equal(["bird.png", "hawk.png"], scene.Atlas);
        Assert.Equal<Shape>(
        [
            new DiscShape(5, 6, 7, 0x00FF00FF, 1),
            new RectShape(10, 20, 30, 40, 0.5, 0xFF0000FF, 2),
            new SpriteShape(1, 8, 9, 10, 11, -0.5, 0xFFFFFFFF, 2),
            new StripShape([(0, 1), (2, 3)], 0x000000FF, 2.5, 4),
            new GlyphsShape("hello", 1, 2, 12, 0x11223344, 8),
        ], scene.Shapes, ShapeComparer.Instance);
    }

    [Fact]
    public void StreamedShapesFollowTheNodesOwnAtEqualDepth()
    {
        var scene = Decode("""{"width":10,"height":10,"shapes":[[1,9,9,1,65535,4]],"streamed":2}""",
            new Streamed(1, 0, 1, 2, 3, 0, 0, 255, 4),
            new Streamed(0, 0, 4, 5, 6, 7, 0, 511, 0));
        Assert.Equal([0.0, 4, 4], scene.Shapes.Select(shape => shape.Depth));
        Assert.Equal(new RectShape(4, 5, 6, 7, 0, 511, 0), scene.Shapes[0]);
        Assert.Equal(new DiscShape(9, 9, 1, 65535, 4), scene.Shapes[1]);
        Assert.Equal(new DiscShape(1, 2, 3, 255, 4), scene.Shapes[2]);
    }

    [Fact]
    public void APathDecodesItsStepsOutlinePaintAndMove()
    {
        var scene = Decode("""{"width":0,"height":0,"shapes":[[5,[[0,1,2],[1,3,4],[2,5,6,7,8],[3,1,2,3,4,5,6],[4,9,9,0.5,1,0,10,11],[5,1,0,0,4,0,4,4],[6]],[1,2.5,1,4,2],[1,180,[1,0.6666666667,1,0.5,1],0.4,[0,1,0,0,1],1,1],[5,6,2],3]]}""");
        var path = Assert.IsType<PathShape>(Assert.Single(scene.Shapes));
        Assert.Equal<PathStep>(
        [
            new MoveTo(1, 2), new LineTo(3, 4), new CurveTo(5, 6, 7, 8), new CubicTo(1, 2, 3, 4, 5, 6),
            new ArcTo(9, 9, 0.5, true, false, 10, 11),
        ], path.Steps.Take(5));
        Assert.Equal([(0.0, 0.0), (4, 0), (4, 4)], Assert.IsType<Polygon>(path.Steps[5]).Corners);
        Assert.IsType<Close>(path.Steps[6]);
        Assert.Equal((true, 2.5, true), (path.Outline.Stroked, path.Outline.Width, path.Outline.Bevel));
        Assert.Equal([4.0, 2], path.Outline.Dashes);
        var paint = Assert.IsType<LinearPaint>(path.Paint);
        Assert.Equal((180.0, 0.4, 1.0, true), (paint.Angle, paint.FromAt, paint.ToAt, paint.Oklab));
        Assert.Equal(new Tint(true, 0.6666666667, 1, 0.5, 1), paint.From);
        Assert.Equal((5.0, 6.0, 2.0), path.Moved);
        Assert.Equal(3, path.Depth);
    }

    [Theory]
    [InlineData("""{"width":10,"height":0,"shapes":[]}""")]
    [InlineData("""{"width":10,"height":10,"shapes":[[0,1,2]]}""")]
    [InlineData("""{"width":10,"height":10,"shapes":[[9,1]]}""")]
    [InlineData("""{"width":10,"height":10,"shapes":[[2,3,0,0,1,1,255,1,0]]}""")]
    [InlineData("""{"width":10,"height":10,"shapes":[[4,"two\nlines",0,0,12,255,0]]}""")]
    [InlineData("""{"width":10,"height":10,"shapes":[[1,0,0,1,-1,0]]}""")]
    public void AMalformedSceneIsRefused(string node) => Assert.Throws<HostException>(() => Decode(node));

    [Fact]
    public void AStreamedCodeOutsideRectDiscAndSpriteIsRefused() =>
        Assert.Throws<HostException>(() => Decode("""{"width":10,"height":10,"shapes":[],"streamed":1}""", new Streamed(2, 0, 0, 0, 0, 0, 0, 0, 0)));

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
