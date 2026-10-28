using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Osprey.Host.WinUI;

/// <summary>
/// Semantic roles and tones resolved against the system's own theme resources [UI-THEME], so the
/// application looks like Windows in light, dark and high-contrast appearance.
/// </summary>
sealed partial class Renderer
{
    /// <summary>The kind-to-control table; its keys must equal <see cref="Coverage.Drawn"/>.</summary>
    Dictionary<string, Func<Scope, JsonElement, UIElement>> Kinds() => new()
    {
        ["text"] = (_, node) => Written(node), ["label"] = (_, node) => Labelled(node), ["pair"] = (_, node) => Pair(node),
        ["badge"] = (_, node) => Badge(node), ["icon"] = (_, node) => Icon(node), ["link"] = (_, node) => Link(node),
        ["spacer"] = (_, _) => new Border(), ["divider"] = (_, _) => Divider(), ["segments"] = (_, node) => Segments(node),
        ["empty"] = (_, node) => Empty(node), ["progress"] = (_, node) => Progress(node), ["meter"] = (_, node) => Meter(node),
        ["swatch"] = (_, node) => Swatch(node), ["slot"] = Slot, ["stack"] = Stack, ["section"] = Section,
        ["screen"] = Screen, ["split"] = Split, ["layer"] = Layer, ["context-menu"] = ContextMenu, ["aside"] = Aside,
        ["tabs"] = Tabs, ["rows"] = Rows, ["button"] = Button, ["field"] = Field, ["toggle"] = Toggle,
        ["picker"] = Picker, ["disclosure"] = Disclosure, ["alert"] = Alert,
    };

    static Brush Brush(string resource) => (Brush)Application.Current.Resources[resource];

    static Style Style(string resource) => (Style)Application.Current.Resources[resource];

    /// <summary>The brush for a tone; the primary tone is the theme's text colour.</summary>
    static Brush Tone(string tone) => Brush(tone switch
    {
        "secondary" => "TextFillColorSecondaryBrush",
        "accent" => "AccentTextFillColorPrimaryBrush",
        "good" => "SystemFillColorSuccessBrush",
        "caution" => "SystemFillColorCautionBrush",
        "danger" => "SystemFillColorCriticalBrush",
        _ => "TextFillColorPrimaryBrush",
    });

    /// <summary>The Windows type ramp style for a text role.</summary>
    static Style Role(string role) => Style(role switch
    {
        "display" => "DisplayTextBlockStyle",
        "large" => "TitleLargeTextBlockStyle",
        "title" => "TitleTextBlockStyle",
        "heading" or "lead" => "SubtitleTextBlockStyle",
        "headline" => "BodyStrongTextBlockStyle",
        "caption" or "eyebrow" => "CaptionTextBlockStyle",
        _ => "BodyTextBlockStyle",
    });

    TextBlock Words(string text, string role = "body", string tone = "primary")
    {
        var block = new TextBlock { Text = text, Style = Role(role), Foreground = Tone(tone), TextWrapping = TextWrapping.Wrap };
        if (role == "mono")
        {
            block.FontFamily = new FontFamily("Cascadia Mono, Consolas");
        }
        return block;
    }

    /// <summary>A symbol as a Segoe Fluent Icons glyph; an unknown name is refused, an empty one draws nothing.</summary>
    UIElement Glyph(string name, double size = 16)
    {
        var glyph = Glyphs.For(name);
        if (glyph is null)
        {
            return Refused($"This host has no icon for the symbol {name}");
        }
        var icon = new FontIcon { Glyph = glyph, FontSize = size };
        AutomationProperties.SetAccessibilityView(icon, Microsoft.UI.Xaml.Automation.Peers.AccessibilityView.Raw);
        return icon;
    }

    static IconElement? IconOf(string name) => Glyphs.For(name) is { Length: > 0 } glyph ? new FontIcon { Glyph = glyph } : null;

    StackPanel Line(params UIElement[] parts)
    {
        var line = new StackPanel { Orientation = Orientation.Horizontal, Spacing = Gap(1) };
        foreach (var part in parts)
        {
            line.Children.Add(part);
        }
        return line;
    }

    StackPanel Column(IEnumerable<UIElement> parts, int gap = 2)
    {
        var column = new StackPanel { Spacing = Gap(gap) };
        foreach (var part in parts)
        {
            column.Children.Add(part);
        }
        return column;
    }
}
