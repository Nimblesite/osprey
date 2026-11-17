using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Documents;
using Microsoft.UI.Xaml.Media;

namespace Osprey.Host.WinUI;

/// <summary>
/// Presentation nodes [UI-COMPONENTS]: a <c>framed</c> body drawn on the surface its style names,
/// and <c>rich-text</c>, inline runs wrapping as one paragraph. Every colour is a token of the
/// nearest theme, so the host holds no colour of its own; a token the theme lacks draws nothing.
/// </summary>
sealed partial class Renderer
{
    // The fixed-width face code runs and mono frames use, the same one the host's other text uses.
    static readonly FontFamily Mono = new("Cascadia Mono, Consolas");

    // The theme frames and runs resolve their tokens against: the application's, or the nearest themed node's.
    Theme? palette;

    static bool Dark => Application.Current.RequestedTheme == ApplicationTheme.Dark;

    Brush? Paint(string token) =>
        token.Length > 0 && Token(palette, token, Dark) is { } colour ? new SolidColorBrush(colour) : null;

    /// <summary>Draws <paramref name="draw"/> with <paramref name="theme"/> as the palette, then restores the outer one.</summary>
    T Under<T>(Theme? theme, Func<T> draw)
    {
        var outer = palette;
        palette = theme ?? outer;
        try
        {
            return draw();
        }
        finally
        {
            palette = outer;
        }
    }

    /// <summary>
    /// The body on its surface: fill, a 1pt line on every edge or one, a corner radius and insets,
    /// a fixed or growing size, the body's place on both axes, its ink and face, and, when the style
    /// asks, a vertical scroll that follows the end of a frame lined up from its end.
    /// </summary>
    UIElement Framed(Scope scope, Node node)
    {
        var style = node.Member("style");
        var content = node.Member("body");
        var body = (FrameworkElement)Draw(scope, content);
        Placed(body, style, (style.Flag("grow") || style.Number("height") > 0) && Fills(content));
        var inked = Inked(body, style);
        var card = new Border
        {
            Background = Paint(style.Text("fill")),
            CornerRadius = new CornerRadius(style.Number("radius")),
            Child = style.Flag("scrollY") ? Scrolling(Padded(inked, style), style) : inked,
            Padding = style.Flag("scrollY") ? new Thickness(0) : Insets(style),
        };
        Lined(card, style);
        Sized(card, style);
        Lifted(card, style);
        return card;
    }

    static Thickness Insets(Node style) =>
        new(style.Number("left"), style.Number("top"), style.Number("right"), style.Number("bottom"));

    static Border Padded(UIElement content, Node style) => new() { Child = content, Padding = Insets(style) };

    // The body's place in the frame: `align` on the vertical axis, `cross` across it.
    static void Placed(FrameworkElement body, Node style, bool fills)
    {
        body.VerticalAlignment = style.Text("align") switch
        {
            "center" => VerticalAlignment.Center,
            "end" => VerticalAlignment.Bottom,
            _ => fills ? VerticalAlignment.Stretch : VerticalAlignment.Top,
        };
        body.HorizontalAlignment = style.Text("cross") switch
        {
            "center" => HorizontalAlignment.Center,
            "end" => HorizontalAlignment.Right,
            "start" => HorizontalAlignment.Left,
            _ => HorizontalAlignment.Stretch,
        };
    }

    // The ink and face the body's words inherit; a word with a tone of its own keeps it.
    ContentControl Inked(UIElement body, Node style)
    {
        var inked = new ContentControl
        {
            Content = body,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            VerticalContentAlignment = VerticalAlignment.Stretch,
            IsTabStop = false,
            FontStyle = style.Flag("italic") ? Windows.UI.Text.FontStyle.Italic : Windows.UI.Text.FontStyle.Normal,
        };
        if (Paint(style.Text("ink")) is { } ink)
        {
            inked.Foreground = ink;
        }
        if (Face(style.Text("font")) is { } face)
        {
            inked.FontFamily = face;
        }
        if (style.Number("textSize") > 0)
        {
            inked.FontSize = style.Number("textSize");
        }
        return inked;
    }

    static FontFamily? Face(string font) => font switch
    {
        "mono" => Mono,
        "serif" => new FontFamily("Georgia, Cambria"),
        "sans" => new FontFamily("Segoe UI Variable Text, Segoe UI"),
        _ => null,
    };

    // A frame lined up from its end follows new content, as a conversation does.
    static ScrollViewer Scrolling(UIElement content, Node style) => new()
    {
        Content = content,
        VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
        HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled,
        VerticalAnchorRatio = style.Text("align") == "end" ? 1 : 0,
    };

    void Lined(Border card, Node style)
    {
        if (Paint(style.Text("line")) is not { } line)
        {
            return;
        }
        card.BorderBrush = line;
        card.BorderThickness = style.Text("lineEdge") switch
        {
            "top" => new Thickness(0, 1, 0, 0),
            "bottom" => new Thickness(0, 0, 0, 1),
            "left" => new Thickness(1, 0, 0, 0),
            "right" => new Thickness(0, 0, 1, 0),
            _ => new Thickness(1),
        };
    }

    // A width is the most a frame takes, so a column narrower than it shrinks the frame; a row fixes it (Stack).
    // A height is kept, and a frame that hugs its content keeps to its natural width.
    static void Sized(Border card, Node style)
    {
        if (style.Number("width") > 0)
        {
            card.MaxWidth = style.Number("width");
        }
        if (style.Number("height") > 0)
        {
            card.Height = style.Number("height");
        }
        if (style.Flag("hug"))
        {
            card.HorizontalAlignment = HorizontalAlignment.Left;
        }
    }

    // The floating shadow a frame asks for, cast by its surface.
    static void Lifted(Border card, Node style)
    {
        if (style.Text("shadow") == "floating")
        {
            card.Shadow = new ThemeShadow();
            card.Translation = new System.Numerics.Vector3(0, 0, 32);
        }
    }

    /// <summary>A frame that takes the room left over along its stack: one that grows and has no fixed height.</summary>
    static bool Grows(Node node) =>
        node.Text("kind") == "framed" && node.Member("style").Flag("grow") && node.Member("style").Number("height") == 0;

    /// <summary>Inline runs wrapping as one paragraph, each with its emphasis and its ink token; selectable, as body text is.</summary>
    UIElement Rich(Node node)
    {
        var block = new TextBlock { Style = Role(node.Text("role")), TextWrapping = TextWrapping.Wrap, IsTextSelectionEnabled = true };
        foreach (var run in node.Items("runs"))
        {
            block.Inlines.Add(Piece(run));
        }
        return block;
    }

    Run Piece(Node run)
    {
        var piece = new Run { Text = run.Text("text") };
        switch (run.Text("emphasis"))
        {
            case "strong":
                piece.FontWeight = FontWeights.SemiBold;
                break;
            case "code":
                piece.FontFamily = Mono;
                break;
            default:
                piece.FontStyle = run.Flag("italic") ? Windows.UI.Text.FontStyle.Italic : Windows.UI.Text.FontStyle.Normal;
                break;
        }
        if (Paint(run.Text("ink")) is { } ink)
        {
            piece.Foreground = ink;
        }
        return piece;
    }
}
