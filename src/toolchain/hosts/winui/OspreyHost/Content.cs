using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Osprey.Host.WinUI;

/// <summary>Content nodes: text, labels, values, symbols, links, rules, states and bars [UI-COMPONENTS].</summary>
sealed partial class Renderer
{
    UIElement Written(Node node) => Words(node.Text("text"), node.Text("role"), node.Text("tone"));

    UIElement Labelled(Node node) => Line(Glyph(node.Text("symbol")), Words(node.Text("label")));

    UIElement Pair(Node node)
    {
        var grid = new Grid { ColumnSpacing = Gap(2) };
        Place(grid, Words(node.Text("label"), "body", "secondary"), across: true, Rest);
        Place(grid, Words(node.Text("value")), across: true, GridLength.Auto);
        return grid;
    }

    /// <summary>A pill whose text and symbol carry the meaning, so colour is never the only signal.</summary>
    UIElement Badge(Node node) => new Border
    {
        CornerRadius = new CornerRadius(10),
        Padding = new Thickness(8, 2, 8, 2),
        BorderThickness = new Thickness(1),
        BorderBrush = Tone(node.Text("tone")),
        HorizontalAlignment = HorizontalAlignment.Left,
        Child = Line(Glyph(node.Text("symbol"), 12), Words(node.Text("label"), "caption", node.Text("tone"))),
    };

    UIElement Icon(Node node) => Glyph(node.Text("symbol"), 32);

    /// <summary>Only an https page opens; any other address is refused where the link would be.</summary>
    UIElement Link(Node node) =>
        Uri.TryCreate(node.Text("url"), UriKind.Absolute, out var page) && page.Scheme == Uri.UriSchemeHttps
            ? new HyperlinkButton { Content = node.Text("label"), NavigateUri = page }
            : Refused($"The link {node.Text("label")} is not an https page");

    UIElement Divider() => new Border { Height = 1, Background = Brush("DividerStrokeColorDefaultBrush") };

    UIElement Segments(Node node)
    {
        var bar = new Grid { Height = 8, ColumnSpacing = 2 };
        foreach (var tone in node.Texts("tones"))
        {
            var piece = new Border { Background = Tone(tone), CornerRadius = new CornerRadius(2) };
            Grid.SetColumn(piece, bar.ColumnDefinitions.Count);
            bar.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            bar.Children.Add(piece);
        }
        AutomationProperties.SetName(bar, node.Text("label"));
        return Column([bar, Words(node.Text("label"), "caption", "secondary")], 0);
    }

    UIElement Empty(Node node)
    {
        var column = Column([Glyph(node.Text("symbol"), 40), Words(node.Text("title"), "heading"), Words(node.Text("detail"), "body", "secondary")]);
        column.HorizontalAlignment = HorizontalAlignment.Center;
        column.VerticalAlignment = VerticalAlignment.Center;
        column.Padding = new Thickness(Gap(4));
        return column;
    }

    UIElement Progress(Node node) =>
        Column([new ProgressBar { IsIndeterminate = true }, Words(node.Text("label")), Words(node.Text("detail"), "caption", "secondary")], 1);

    /// <summary>[UI-COMPONENTS-METER] A determinate bar filled to value over total, read out by its label.</summary>
    UIElement Meter(Node node)
    {
        var bar = new ProgressBar { Maximum = Math.Max(1, node.Number("total")), Value = node.Number("value"), Foreground = Tone(node.Text("tone")), MinWidth = 60 };
        AutomationProperties.SetName(bar, node.Text("label"));
        return bar;
    }

    UIElement Swatch(Node node)
    {
        var dark = Application.Current.RequestedTheme == ApplicationTheme.Dark;
        var chip = new Border { Width = 28, Height = 28, CornerRadius = new CornerRadius(6), Background = new SolidColorBrush(Colour(node.Text(dark ? "dark" : "light"))) };
        return Line(chip, Words(node.Text("name")));
    }

    static Windows.UI.Color Colour(string hex) =>
        hex.Length == 7 && hex[0] == '#' && int.TryParse(hex[1..], System.Globalization.NumberStyles.HexNumber, null, out var rgb)
            ? Windows.UI.Color.FromArgb(255, (byte)(rgb >> 16), (byte)(rgb >> 8), (byte)rgb)
            : throw new HostException($"The swatch colour {hex} is not #RRGGBB");
}
