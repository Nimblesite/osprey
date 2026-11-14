using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using XamlControl = Microsoft.UI.Xaml.Controls.Control;

namespace Osprey.Host.WinUI;

/// <summary>
/// The look of every control [NATIVE-SHM-VIEW], applied only from the values its row carries: the
/// page holds no colour, font, padding or spacing of its own. The one exception is structural: a
/// sheet's veil is transparent so it catches the tap that dismisses it.
/// </summary>
sealed partial class SharedViewPage
{
    // The fixed-width face a row's monospaced flag asks for, the same one the host's other text uses.
    static readonly FontFamily FixedWidth = new("Cascadia Mono, Consolas");

    // A sheet's veil: transparent, so it blocks what floats below and catches the dismissing tap.
    static readonly Brush SeeThrough = new SolidColorBrush(Windows.UI.Color.FromArgb(0, 0, 0, 0));

    // 0xRRGGBBAA as the rows carry it; a zero draws nothing, so it answers no brush at all.
    static Brush? Paint(uint rgba) => rgba == 0 ? null : new SolidColorBrush(Shade(rgba));

    static Windows.UI.Color Shade(uint rgba) =>
        Windows.UI.Color.FromArgb((byte)rgba, (byte)(rgba >> 24), (byte)(rgba >> 16), (byte)(rgba >> 8));

    static Windows.UI.Text.FontWeight Heft(double weight) => new() { Weight = (ushort)Math.Clamp(weight, 100, 900) };

    // The letters of a text-bearing control, every trait from its row: ink, size, heft, slant and
    // face. Two overloads exist because a TextBlock is not a Control.
    static void Lettered(XamlControl control, Control row)
    {
        control.Foreground = Paint(row.Look.Ink);
        if (row.Look.Size > 0) control.FontSize = row.Look.Size;
        control.FontWeight = Heft(row.Look.Weight);
        control.FontStyle = row.Fit.Slanted ? Windows.UI.Text.FontStyle.Italic : Windows.UI.Text.FontStyle.Normal;
        control.FontFamily = row.Fit.Monospaced ? FixedWidth : null;
    }

    static void Lettered(TextBlock block, Control row)
    {
        block.Foreground = Paint(row.Look.Ink);
        if (row.Look.Size > 0) block.FontSize = row.Look.Size;
        block.FontWeight = Heft(row.Look.Weight);
        block.FontStyle = row.Fit.Slanted ? Windows.UI.Text.FontStyle.Italic : Windows.UI.Text.FontStyle.Normal;
        block.FontFamily = row.Fit.Monospaced ? FixedWidth : null;
    }

    // The face of a control or of the card wrapping one: fill, corner, inset and its 1pt edge.
    static void Faced(Border card, Control row)
    {
        card.Background = Paint(row.Look.Fill);
        card.CornerRadius = new CornerRadius(row.Look.Radius);
        card.Padding = new Thickness(row.Look.Padding);
        Edged(card, row);
    }

    static void Faced(XamlControl control, Control row)
    {
        control.IsEnabled = !row.Fit.Disabled;
        control.Background = Paint(row.Look.Fill);
        control.CornerRadius = new CornerRadius(row.Look.Radius);
        control.Padding = new Thickness(row.Look.Padding);
        Edged(control, row);
    }

    // A border colour draws a one-point edge; no colour draws none. A Border holds the edge of a
    // wrapped control and a Control its own.
    static void Edged(FrameworkElement edged, Control row)
    {
        var edge = Paint(row.Fit.Border);
        var thick = new Thickness(row.Fit.Border == 0 ? 0 : 1);
        switch (edged)
        {
            case Border card:
                card.BorderBrush = edge;
                card.BorderThickness = thick;
                break;
            case XamlControl control:
                control.BorderBrush = edge;
                control.BorderThickness = thick;
                break;
        }
    }

    // How a control sits: the fixed extents its row names (zero keeps its natural ones) and, when
    // the row centres it, that.
    static void Measured(FrameworkElement element, Control row)
    {
        element.Width = row.Fit.Width > 0 ? row.Fit.Width : double.NaN;
        element.Height = row.Fit.Height > 0 ? row.Fit.Height : double.NaN;
        if (row.Fit.Centred) element.HorizontalAlignment = HorizontalAlignment.Center;
    }
}
