using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using static Osprey.Host.Envelopes;

namespace Osprey.Host.WinUI;

/// <summary>Content nodes: text, labels, values, symbols, links, rules, states and bars [UI-COMPONENTS].</summary>
sealed partial class Renderer
{
    UIElement Written(JsonElement node) => Words(Text(node, "text"), Text(node, "role"), Text(node, "tone"));

    UIElement Labelled(JsonElement node) => Line(Glyph(Text(node, "symbol")), Words(Text(node, "label")));

    UIElement Pair(JsonElement node)
    {
        var grid = new Grid { ColumnSpacing = Gap(2) };
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        var value = Words(Text(node, "value"));
        Grid.SetColumn(value, 1);
        grid.Children.Add(Words(Text(node, "label"), "body", "secondary"));
        grid.Children.Add(value);
        return grid;
    }

    /// <summary>A pill whose text and symbol carry the meaning, so colour is never the only signal.</summary>
    UIElement Badge(JsonElement node) => new Border
    {
        CornerRadius = new CornerRadius(10),
        Padding = new Thickness(8, 2, 8, 2),
        BorderThickness = new Thickness(1),
        BorderBrush = Tone(Text(node, "tone")),
        HorizontalAlignment = HorizontalAlignment.Left,
        Child = Line(Glyph(Text(node, "symbol"), 12), Words(Text(node, "label"), "caption", Text(node, "tone"))),
    };

    UIElement Icon(JsonElement node) => Glyph(Text(node, "symbol"), 32);

    /// <summary>Only an https page opens; any other address is refused where the link would be.</summary>
    UIElement Link(JsonElement node) =>
        Uri.TryCreate(Text(node, "url"), UriKind.Absolute, out var page) && page.Scheme == Uri.UriSchemeHttps
            ? new HyperlinkButton { Content = Text(node, "label"), NavigateUri = page }
            : Refused($"The link {Text(node, "label")} is not an https page");

    UIElement Divider() => new Border { Height = 1, Background = Brush("DividerStrokeColorDefaultBrush") };

    UIElement Segments(JsonElement node)
    {
        var bar = new Grid { Height = 8, ColumnSpacing = 2 };
        foreach (var tone in Items(node, "tones", item => item.GetString() ?? ""))
        {
            var piece = new Border { Background = Tone(tone), CornerRadius = new CornerRadius(2) };
            Grid.SetColumn(piece, bar.ColumnDefinitions.Count);
            bar.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            bar.Children.Add(piece);
        }
        AutomationProperties.SetName(bar, Text(node, "label"));
        return Column([bar, Words(Text(node, "label"), "caption", "secondary")], 0);
    }

    UIElement Empty(JsonElement node)
    {
        var column = Column([Glyph(Text(node, "symbol"), 40), Words(Text(node, "title"), "heading"), Words(Text(node, "detail"), "body", "secondary")]);
        column.HorizontalAlignment = HorizontalAlignment.Center;
        column.VerticalAlignment = VerticalAlignment.Center;
        column.Padding = new Thickness(Gap(4));
        return column;
    }

    UIElement Progress(JsonElement node) =>
        Column([new ProgressBar { IsIndeterminate = true }, Words(Text(node, "label")), Words(Text(node, "detail"), "caption", "secondary")], 1);

    /// <summary>[UI-COMPONENTS-METER] A determinate bar filled to value over total, read out by its label.</summary>
    UIElement Meter(JsonElement node)
    {
        var bar = new ProgressBar { Maximum = Math.Max(1, Number(node, "total")), Value = Number(node, "value"), Foreground = Tone(Text(node, "tone")), MinWidth = 60 };
        AutomationProperties.SetName(bar, Text(node, "label"));
        return bar;
    }

    UIElement Swatch(JsonElement node)
    {
        var dark = Application.Current.RequestedTheme == ApplicationTheme.Dark;
        var chip = new Border { Width = 28, Height = 28, CornerRadius = new CornerRadius(6), Background = new SolidColorBrush(Colour(Text(node, dark ? "dark" : "light"))) };
        return Line(chip, Words(Text(node, "name")));
    }

    static Windows.UI.Color Colour(string hex) =>
        hex.Length == 7 && hex[0] == '#' && int.TryParse(hex[1..], System.Globalization.NumberStyles.HexNumber, null, out var rgb)
            ? Windows.UI.Color.FromArgb(255, (byte)(rgb >> 16), (byte)(rgb >> 8), (byte)rgb)
            : throw new HostException($"The swatch colour {hex} is not #RRGGBB");
}
