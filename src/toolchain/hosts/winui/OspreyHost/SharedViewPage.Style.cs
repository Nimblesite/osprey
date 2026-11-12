using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

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
    static Brush? Paint(long rgba) => rgba == 0 ? null : new SolidColorBrush(Shade(rgba));

    static Windows.UI.Color Shade(long rgba) =>
        Windows.UI.Color.FromArgb((byte)rgba, (byte)(rgba >> 24), (byte)(rgba >> 16), (byte)(rgba >> 8));

    static Windows.UI.Text.FontWeight Heft(int weight) => new() { Weight = (ushort)Math.Clamp(weight, 100, 900) };

    // The letters of a text-bearing control, every trait from its row: ink, size, heft, slant and
    // face. Two overloads exist because a TextBlock is not a Control.
    static void Lettered(Control control, SafeRow row)
    {
        control.Foreground = Paint(row.Colour);
        if (row.Size > 0) control.FontSize = row.Size;
        control.FontWeight = Heft(row.Weight);
        control.FontStyle = row.Slanted ? Windows.UI.Text.FontStyle.Italic : Windows.UI.Text.FontStyle.Normal;
        control.FontFamily = row.Monospaced ? FixedWidth : null;
    }

    static void Lettered(TextBlock block, SafeRow row)
    {
        block.Foreground = Paint(row.Colour);
        if (row.Size > 0) block.FontSize = row.Size;
        block.FontWeight = Heft(row.Weight);
        block.FontStyle = row.Slanted ? Windows.UI.Text.FontStyle.Italic : Windows.UI.Text.FontStyle.Normal;
        block.FontFamily = row.Monospaced ? FixedWidth : null;
    }

    // The face of a control or of the card wrapping one: fill, corner, inset and its 1pt edge.
    static void Faced(Border card, SafeRow row)
    {
        card.Background = Paint(row.Background);
        card.CornerRadius = new CornerRadius(row.Radius);
        card.Padding = new Thickness(row.Padding);
        Edged(card, row);
    }

    static void Faced(Control control, SafeRow row)
    {
        control.IsEnabled = !row.Disabled;
        control.Background = Paint(row.Background);
        control.CornerRadius = new CornerRadius(row.Radius);
        control.Padding = new Thickness(row.Padding);
        Edged(control, row);
    }

    // A border colour draws a one-point edge; no colour draws none. A Border holds the edge of a
    // wrapped control and a Control its own.
    static void Edged(FrameworkElement edged, SafeRow row)
    {
        var edge = Paint(row.Border);
        var thick = new Thickness(row.Border == 0 ? 0 : 1);
        switch (edged)
        {
            case Border card:
                card.BorderBrush = edge;
                card.BorderThickness = thick;
                break;
            case Control control:
                control.BorderBrush = edge;
                control.BorderThickness = thick;
                break;
        }
    }

    // How a control sits: the fixed extents its row names (zero keeps its natural ones) and, when
    // the row centres it, that.
    static void Measured(FrameworkElement element, SafeRow row)
    {
        element.Width = row.Width > 0 ? row.Width : double.NaN;
        element.Height = row.Height > 0 ? row.Height : double.NaN;
        if (row.Centred) element.HorizontalAlignment = HorizontalAlignment.Center;
    }
}
